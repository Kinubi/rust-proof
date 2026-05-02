use futures::Future;

use socket2::SockAddr;

use crate::runtime::errors::RuntimeError;

pub trait SocketFactory {
    type TcpStream;
    type TcpListener;

    fn bind(
        &self,
        port: u16
    ) -> impl Future<Output = Result<Self::TcpListener, RuntimeError>> + Send;
    fn accept(
        &self,
        listener: &mut Self::TcpListener
    ) -> impl Future<Output = Result<(Self::TcpStream, SockAddr), RuntimeError>> + Send;
    fn connect(
        &self,
        addr: SockAddr
    ) -> impl Future<Output = Result<Self::TcpStream, RuntimeError>> + Send;
}
