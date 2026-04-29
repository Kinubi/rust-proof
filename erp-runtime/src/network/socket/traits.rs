pub trait SocketFactory {
    type TcpStream;
    type TcpListener;

    async fn bind(&self, port: u16) -> Result<Self::TcpListener, RuntimeError>;
    async fn accept(
        &self,
        listener: &mut Self::TcpListener
    ) -> Result<(Self::TcpStream, std::net::SocketAddr), RuntimeError>;
    async fn connect(&self, addr: std::net::SocketAddr) -> Result<Self::TcpStream, RuntimeError>;
}
