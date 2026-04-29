use std::net::SocketAddr;

// The Wi-Fi/netif stack is owned by connection bring-up, not by each socket.
pub struct EspSocket {
    pub addr: SocketAddr,
}
