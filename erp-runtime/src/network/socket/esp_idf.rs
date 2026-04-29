use std::{ io::ErrorKind, net::{ SocketAddr, TcpListener, TcpStream } };

use embassy_time::{ Duration, Timer };
use socket2::{ Domain, Protocol, SockAddr, Socket, Type };

use crate::{ network::socket::traits::SocketFactory, runtime::errors::RuntimeError };

const SOCKET_RETRY_DELAY_MS: u64 = 150;

// The Wi-Fi/netif stack is owned by connection bring-up, not by each socket.
pub struct EspSocketFactory {
    pub addr: SocketAddr,
}

pub struct EspTcpListener {
    listener: TcpListener,
}
pub struct EspTcpStream {
    stream: TcpStream,
}

impl EspTcpStream {
    pub fn stream(&self) -> &TcpStream {
        &self.stream
    }

    pub fn stream_mut(&mut self) -> &mut TcpStream {
        &mut self.stream
    }

    pub fn into_inner(self) -> TcpStream {
        self.stream
    }
}

impl EspSocketFactory {
    pub fn new(addr: SocketAddr) -> Self {
        Self { addr }
    }
}

impl SocketFactory for EspSocketFactory {
    type TcpListener = EspTcpListener;
    type TcpStream = EspTcpStream;

    async fn bind(&self, port: u16) -> Result<Self::TcpListener, RuntimeError> {
        let listener = TcpListener::bind(SocketAddr::new(self.addr.ip(), port)).map_err(
            RuntimeError::NetworkError
        )?;

        listener.set_nonblocking(true).map_err(RuntimeError::NetworkError)?;

        Ok(EspTcpListener { listener })
    }
    async fn accept(
        &self,
        listener: &mut Self::TcpListener
    ) -> Result<(Self::TcpStream, std::net::SocketAddr), RuntimeError> {
        loop {
            match listener.listener.accept() {
                Ok((stream, peer_addr)) => {
                    stream.set_nonblocking(true).map_err(RuntimeError::NetworkError)?;
                    return Ok((EspTcpStream { stream }, peer_addr));
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

    async fn connect(&self, addr: std::net::SocketAddr) -> Result<Self::TcpStream, RuntimeError> {
        let socket = Socket::new(
            match addr {
                SocketAddr::V4(_) => Domain::IPV4,
                SocketAddr::V6(_) => Domain::IPV6,
            },
            Type::STREAM,
            Some(Protocol::TCP)
        ).map_err(RuntimeError::NetworkError)?;

        socket.set_nonblocking(true).map_err(RuntimeError::NetworkError)?;

        match socket.connect(&SockAddr::from(addr)) {
            Ok(()) => {
                let stream: TcpStream = socket.into();
                return Ok(EspTcpStream { stream });
            }
            Err(error) if
                matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted)
            => {}
            Err(error) => {
                return Err(RuntimeError::NetworkError(error));
            }
        }

        loop {
            if let Some(error) = socket.take_error().map_err(RuntimeError::NetworkError)? {
                return Err(RuntimeError::NetworkError(error));
            }

            match socket.peer_addr() {
                Ok(_) => {
                    let stream: TcpStream = socket.into();
                    return Ok(EspTcpStream { stream });
                }
                Err(error) if error.kind() == ErrorKind::NotConnected => {
                    Timer::after(Duration::from_millis(SOCKET_RETRY_DELAY_MS)).await;
                }
                Err(error) => {
                    return Err(RuntimeError::NetworkError(error));
                }
            }
        }
    }
}
