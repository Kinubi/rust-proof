use std::{
    io::{ErrorKind, Read, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
};

use embassy_time::{Duration, Timer};
use futures::io::AllowStdIo;
use socket2::{Domain, Protocol, SockAddr, Socket, Type};

use crate::{network::socket::traits::SocketFactory, runtime::errors::RuntimeError};

const SOCKET_RETRY_DELAY_MS: u64 = 10;

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

    pub fn into_blocking(self) -> Result<TcpStream, RuntimeError> {
        self.stream
            .set_nonblocking(false)
            .map_err(RuntimeError::NetworkError)?;
        Ok(self.stream)
    }

    pub fn into_futures_io(self) -> Result<AllowStdIo<TcpStream>, RuntimeError> {
        Ok(AllowStdIo::new(self.into_blocking()?))
    }

    pub async fn read_exact_nonblocking(&mut self, buf: &mut [u8]) -> Result<(), RuntimeError> {
        let mut read = 0usize;
        while read < buf.len() {
            match self.stream.read(&mut buf[read..]) {
                Ok(0) => {
                    return Err(RuntimeError::NetworkError(std::io::Error::from(
                        ErrorKind::UnexpectedEof,
                    )));
                }
                Ok(len) => {
                    read += len;
                }
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) =>
                {
                    Timer::after(Duration::from_millis(SOCKET_RETRY_DELAY_MS)).await;
                }
                Err(error) => {
                    return Err(RuntimeError::NetworkError(error));
                }
            }
        }

        Ok(())
    }

    pub async fn write_all_nonblocking(&mut self, buf: &[u8]) -> Result<(), RuntimeError> {
        let mut written = 0usize;
        while written < buf.len() {
            match self.stream.write(&buf[written..]) {
                Ok(0) => {
                    return Err(RuntimeError::NetworkError(std::io::Error::from(
                        ErrorKind::WriteZero,
                    )));
                }
                Ok(len) => {
                    written += len;
                }
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) =>
                {
                    Timer::after(Duration::from_millis(SOCKET_RETRY_DELAY_MS)).await;
                }
                Err(error) => {
                    return Err(RuntimeError::NetworkError(error));
                }
            }
        }

        Ok(())
    }

    pub async fn flush_nonblocking(&mut self) -> Result<(), RuntimeError> {
        loop {
            match self.stream.flush() {
                Ok(()) => {
                    return Ok(());
                }
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) =>
                {
                    Timer::after(Duration::from_millis(SOCKET_RETRY_DELAY_MS)).await;
                }
                Err(error) => {
                    return Err(RuntimeError::NetworkError(error));
                }
            }
        }
    }

    pub fn shutdown(&mut self) -> Result<(), RuntimeError> {
        self.stream
            .shutdown(Shutdown::Both)
            .map_err(RuntimeError::NetworkError)
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

    fn bind(
        &self,
        port: u16,
    ) -> impl std::future::Future<Output = Result<Self::TcpListener, RuntimeError>> + Send {
        let addr = SocketAddr::new(self.addr.ip(), port);

        async move {
            let listener = TcpListener::bind(addr).map_err(RuntimeError::NetworkError)?;
            listener
                .set_nonblocking(true)
                .map_err(RuntimeError::NetworkError)?;
            Ok(EspTcpListener { listener })
        }
    }

    fn accept(
        &self,
        listener: &mut Self::TcpListener,
    ) -> impl std::future::Future<
        Output = Result<(Self::TcpStream, std::net::SocketAddr), RuntimeError>,
    > + Send {
        async move {
            loop {
                match listener.listener.accept() {
                    Ok((stream, peer_addr)) => {
                        stream
                            .set_nonblocking(true)
                            .map_err(RuntimeError::NetworkError)?;
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
    }

    fn connect(
        &self,
        addr: std::net::SocketAddr,
    ) -> impl std::future::Future<Output = Result<Self::TcpStream, RuntimeError>> + Send {
        async move {
            let socket = Socket::new(
                match addr {
                    SocketAddr::V4(_) => Domain::IPV4,
                    SocketAddr::V6(_) => Domain::IPV6,
                },
                Type::STREAM,
                Some(Protocol::TCP),
            )
            .map_err(RuntimeError::NetworkError)?;

            socket
                .set_nonblocking(true)
                .map_err(RuntimeError::NetworkError)?;

            match socket.connect(&SockAddr::from(addr)) {
                Ok(()) => {
                    let stream: TcpStream = socket.into();
                    return Ok(EspTcpStream { stream });
                }
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => {}
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
}
