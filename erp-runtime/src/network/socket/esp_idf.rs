use std::io::{ ErrorKind, Read, Write };
use std::pin::Pin;
use std::task::{ Context, Poll };

use embassy_time::{ Duration, Timer };
use futures::io::{ AsyncRead, AsyncWrite };
use log::{ info, warn };
use socket2::{ Socket, Domain, Type, Protocol, SockAddr };
use futures::Future;

use crate::{ network::socket::traits::SocketFactory, runtime::errors::RuntimeError };

const TAG: &str = "socket";
const SOCKET_RETRY_DELAY_MS: u64 = 10;
const CONNECT_TIMEOUT_SECS: u64 = 5;
const CONNECT_POLL_MS: u64 = 50;

// The Wi-Fi/netif stack is owned by connection bring-up, not by each socket.
pub struct EspSocketFactory {
    pub addr: SockAddr,
}

pub struct EspTcpListener {
    socket: Socket,
}

pub struct EspTcpStream {
    socket: Socket,
    read_retry: Option<Timer>,
    write_retry: Option<Timer>,
    flush_retry: Option<Timer>,
}

impl EspTcpStream {
    pub fn socket(&self) -> &Socket {
        &self.socket
    }

    pub fn socket_mut(&mut self) -> &mut Socket {
        &mut self.socket
    }

    pub fn into_inner(self) -> Socket {
        self.socket
    }

    pub fn shutdown(&mut self) -> Result<(), RuntimeError> {
        self.socket.shutdown(std::net::Shutdown::Both).map_err(RuntimeError::NetworkError)
    }

    fn poll_retry(timer: &mut Option<Timer>, cx: &mut Context<'_>) -> Poll<()> {
        let retry = timer.get_or_insert_with(|| {
            Timer::after(Duration::from_millis(SOCKET_RETRY_DELAY_MS))
        });

        match Pin::new(retry).poll(cx) {
            Poll::Ready(()) => {
                *timer = None;
                Poll::Ready(())
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Check if error means "try again later" - handles both WouldBlock and lwIP's EINPROGRESS
fn is_would_block(e: &std::io::Error) -> bool {
    e.kind() == ErrorKind::WouldBlock || e.raw_os_error() == Some(119)
}

impl AsyncRead for EspTcpStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8]
    ) -> Poll<std::io::Result<usize>> {
        loop {
            match (&mut self.socket).read(buf) {
                Ok(n) => {
                    self.read_retry = None;
                    return Poll::Ready(Ok(n));
                }
                Err(e) if is_would_block(&e) => {
                    match Self::poll_retry(&mut self.read_retry, cx) {
                        Poll::Ready(()) => {
                            continue;
                        }
                        Poll::Pending => {
                            return Poll::Pending;
                        }
                    }
                }
                Err(e) => {
                    self.read_retry = None;
                    return Poll::Ready(Err(e));
                }
            }
        }
    }
}

impl AsyncWrite for EspTcpStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8]
    ) -> Poll<std::io::Result<usize>> {
        loop {
            match (&mut self.socket).write(buf) {
                Ok(n) => {
                    self.write_retry = None;
                    return Poll::Ready(Ok(n));
                }
                Err(e) if is_would_block(&e) => {
                    match Self::poll_retry(&mut self.write_retry, cx) {
                        Poll::Ready(()) => {
                            continue;
                        }
                        Poll::Pending => {
                            return Poll::Pending;
                        }
                    }
                }
                Err(e) => {
                    self.write_retry = None;
                    return Poll::Ready(Err(e));
                }
            }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        loop {
            match (&mut self.socket).flush() {
                Ok(()) => {
                    self.flush_retry = None;
                    return Poll::Ready(Ok(()));
                }
                Err(e) if is_would_block(&e) => {
                    match Self::poll_retry(&mut self.flush_retry, cx) {
                        Poll::Ready(()) => {
                            continue;
                        }
                        Poll::Pending => {
                            return Poll::Pending;
                        }
                    }
                }
                Err(e) => {
                    self.flush_retry = None;
                    return Poll::Ready(Err(e));
                }
            }
        }
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(self.socket.shutdown(std::net::Shutdown::Write))
    }
}

impl EspSocketFactory {
    pub fn new(addr: SockAddr) -> Self {
        Self { addr }
    }
}

impl SocketFactory for EspSocketFactory {
    type TcpListener = EspTcpListener;
    type TcpStream = EspTcpStream;

    fn bind(
        &self,
        port: u16
    ) -> impl Future<Output = Result<Self::TcpListener, RuntimeError>> + Send {
        // Create bind address with the specified port
        let bind_addr = if self.addr.is_ipv4() {
            let socket_addr = self.addr
                .as_socket_ipv4()
                .map(|a| std::net::SocketAddrV4::new(*a.ip(), port))
                .unwrap();
            SockAddr::from(socket_addr)
        } else {
            let socket_addr = self.addr
                .as_socket_ipv6()
                .map(|a| std::net::SocketAddrV6::new(*a.ip(), port, a.flowinfo(), a.scope_id()))
                .unwrap();
            SockAddr::from(socket_addr)
        };

        async move {
            let domain = if bind_addr.is_ipv4() { Domain::IPV4 } else { Domain::IPV6 };
            let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP)).map_err(
                RuntimeError::NetworkError
            )?;

            socket.set_reuse_address(true).map_err(RuntimeError::NetworkError)?;
            socket.bind(&bind_addr).map_err(RuntimeError::NetworkError)?;
            socket.listen(128).map_err(RuntimeError::NetworkError)?;
            socket.set_nonblocking(true).map_err(RuntimeError::NetworkError)?;

            Ok(EspTcpListener { socket })
        }
    }

    fn accept(
        &self,
        listener: &mut Self::TcpListener
    ) -> impl Future<Output = Result<(Self::TcpStream, SockAddr), RuntimeError>> + Send {
        async move {
            loop {
                match listener.socket.accept() {
                    Ok((socket, addr)) => {
                        socket.set_nonblocking(true).map_err(RuntimeError::NetworkError)?;
                        // Disable Nagle's algorithm to send small packets immediately
                        socket.set_nodelay(true).map_err(RuntimeError::NetworkError)?;
                        return Ok((
                            EspTcpStream {
                                socket,
                                read_retry: None,
                                write_retry: None,
                                flush_retry: None,
                            },
                            addr,
                        ));
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        Timer::after(Duration::from_millis(SOCKET_RETRY_DELAY_MS)).await;
                    }
                    Err(error) => {
                        return Err(RuntimeError::NetworkError(error));
                    }
                }
            }
        }
    }

    fn connect(
        &self,
        addr: SockAddr
    ) -> impl Future<Output = Result<Self::TcpStream, RuntimeError>> + Send {
        async move {
            info!(target: TAG, "connect() to {:?}", addr);

            let domain = if addr.is_ipv4() { Domain::IPV4 } else { Domain::IPV6 };
            let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP)).map_err(
                RuntimeError::NetworkError
            )?;

            socket.set_nonblocking(true).map_err(RuntimeError::NetworkError)?;

            match socket.connect(&addr) {
                Ok(()) => {
                    info!(target: TAG, "connected to {:?} (immediate)", addr);
                }
                Err(e) if
                    e.kind() == ErrorKind::WouldBlock ||
                    e.raw_os_error() == Some(115) || // Linux EINPROGRESS
                    e.raw_os_error() == Some(119)
                => {
                    // lwIP EINPROGRESS
                    // Non-blocking connect in progress
                    let deadline =
                        std::time::Instant::now() +
                        std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS);

                    loop {
                        Timer::after(Duration::from_millis(CONNECT_POLL_MS)).await;

                        // Check for socket error using socket2's take_error()
                        match socket.take_error() {
                            Ok(Some(err)) => {
                                warn!(target: TAG, "connect to {:?} failed: {:?}", addr, err.kind());
                                return Err(RuntimeError::NetworkError(err));
                            }
                            Ok(None) => {
                                // No error, check if connected
                                if socket.peer_addr().is_ok() {
                                    info!(target: TAG, "connected to {:?}", addr);
                                    break;
                                }
                            }
                            Err(err) => {
                                warn!(target: TAG, "take_error failed: {:?}", err);
                                return Err(RuntimeError::NetworkError(err));
                            }
                        }

                        if std::time::Instant::now() > deadline {
                            warn!(target: TAG, "connect to {:?} timed out", addr);
                            return Err(
                                RuntimeError::NetworkError(
                                    std::io::Error::new(ErrorKind::TimedOut, "connection timed out")
                                )
                            );
                        }
                    }
                }
                Err(e) => {
                    warn!(target: TAG, "connect to {:?} failed: {:?}", addr, e.kind());
                    return Err(RuntimeError::NetworkError(e));
                }
            }

            // Clear any lingering error state
            if let Ok(Some(err)) = socket.take_error() {
                warn!(target: TAG, "post-connect socket error: {:?}", err);
                return Err(RuntimeError::NetworkError(err));
            }

            // Disable Nagle's algorithm to send small packets immediately
            socket.set_nodelay(true).map_err(RuntimeError::NetworkError)?;

            // Keep socket non-blocking for async I/O
            Ok(EspTcpStream {
                socket,
                read_retry: None,
                write_retry: None,
                flush_retry: None,
            })
        }
    }
}
