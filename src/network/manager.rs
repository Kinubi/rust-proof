// use libp2p::{gossipsub, kad, request_response, swarm::NetworkBehaviour, Swarm};
use crate::node::NodeCommand;
use tokio::sync::mpsc;

// ============================================================================
// TODO: Chapter 8 - Define NetworkBehaviour
// 1. Add libp2p to Cargo.toml with features: tokio, gossipsub, kad, tcp, noise, yamux
// 2. Define a custom `NetworkBehaviour` struct using #[derive(NetworkBehaviour)]
//    that combines gossipsub, kademlia, and request_response.
// ============================================================================

/// Manages P2P network connections and message broadcasting using libp2p.
pub struct NetworkManager {
    // TODO: Chapter 8 - Add the libp2p Swarm here
    // swarm: Swarm<MyCustomBehaviour>,
    
    /// Channel to send commands back to the central Node.
    node_sender: mpsc::Sender<NodeCommand>,
}

impl NetworkManager {
    pub fn new(node_sender: mpsc::Sender<NodeCommand>) -> Self {
        Self {
            node_sender,
        }
    }

    /// Starts the libp2p Swarm event loop.
    pub async fn start(&mut self) {
        // ====================================================================
        // TODO: Chapter 8 - Implement start
        // 1. Loop over `swarm.select_next_some().await`.
        // 2. Handle Gossipsub events (deserialize messages and send to Node).
        // 3. Handle RequestResponse events (fetch blocks and send back).
        // ====================================================================
        println!("NetworkManager started");
    }

    /// Broadcasts a message to the network via Gossipsub.
    // pub async fn broadcast(&mut self, topic: &str, message: NetworkMessage) {
    pub async fn broadcast(&mut self) {
        // ====================================================================
        // TODO: Chapter 8 - Implement broadcast
        // 1. Serialize the message.
        // 2. Publish the serialized bytes to the given Gossipsub topic.
        // ====================================================================
        println!("Broadcasting message to network");
    }
}
