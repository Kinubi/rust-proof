use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
// use crate::network::message::NetworkMessage; // Uncomment when NetworkMessage is defined
use crate::node::NodeCommand;
use tokio::sync::mpsc;

/// Manages P2P network connections and message broadcasting.
pub struct NetworkManager {
    /// A map of connected peers and their write streams.
    // TODO: Chapter 8 - Use tokio::net::tcp::OwnedWriteHalf or similar for the value
    peers: HashMap<SocketAddr, ()>, 
    /// Channel to send commands back to the central Node.
    node_sender: mpsc::Sender<NodeCommand>,
}

impl NetworkManager {
    pub fn new(node_sender: mpsc::Sender<NodeCommand>) -> Self {
        Self {
            peers: HashMap::new(),
            node_sender,
        }
    }

    /// Starts listening for incoming TCP connections on the specified port.
    pub async fn start_listening(&mut self, port: u16) {
        // ====================================================================
        // TODO: Chapter 8 - Implement start_listening
        // 1. Bind a TcpListener to `0.0.0.0:port`.
        // 2. Loop and accept incoming connections.
        // 3. For each connection, spawn a new tokio task to handle it.
        // ====================================================================
        println!("NetworkManager listening on port {}", port);
    }

    /// Connects to a known peer address.
    pub async fn connect_to_peer(&mut self, addr: SocketAddr) {
        // ====================================================================
        // TODO: Chapter 8 - Implement connect_to_peer
        // 1. Attempt to connect to the address using TcpStream::connect.
        // 2. If successful, add the peer to the `peers` map.
        // 3. Send an initial Handshake message.
        // 4. Spawn a task to read incoming messages from this peer.
        // ====================================================================
        println!("Attempting to connect to peer: {}", addr);
    }

    /// Broadcasts a message to all connected peers.
    // pub async fn broadcast(&mut self, message: NetworkMessage) {
    pub async fn broadcast(&mut self) {
        // ====================================================================
        // TODO: Chapter 8 - Implement broadcast
        // 1. Serialize the message.
        // 2. Iterate over all connected peers.
        // 3. Write the serialized bytes to each peer's stream.
        // ====================================================================
        println!("Broadcasting message to {} peers", self.peers.len());
    }
}
