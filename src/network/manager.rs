use libp2p::{ Swarm, gossipsub, kad, noise, request_response, swarm::NetworkBehaviour, tcp, yamux };
use crate::network::message::NetworkMessage;
use tokio::sync::mpsc;
use crate::node::NodeCommand;

// ============================================================================
// TODO: Chapter 8 - Define NetworkBehaviour
// 1. Define a custom `NetworkBehaviour` struct using #[derive(NetworkBehaviour)]
//    that combines gossipsub, kademlia, and request_response.
// ============================================================================

#[derive(NetworkBehaviour)]
pub struct AppBehaviour {
    pub gossipsub: gossipsub::Behaviour,
    pub kademlia: kad::Behaviour<kad::store::MemoryStore>,
    pub request_response: request_response::json::Behaviour<NetworkMessage, NetworkMessage>,
}

/// Manages P2P network connections and message broadcasting using libp2p.
pub struct NetworkManager {
    // TODO: Chapter 8 - Add the libp2p Swarm here
    swarm: Swarm<AppBehaviour>,

    /// Channel to send commands back to the central Node.
    node_sender: mpsc::Sender<NodeCommand>,
}

impl NetworkManager {
    pub fn new(node_sender: mpsc::Sender<NodeCommand>) -> Self {
        let mut swarm = libp2p::SwarmBuilder
            ::with_new_identity()
            .with_tokio()
            .with_tcp(tcp::Config::default(), noise::Config::new, yamux::Config::default)
            .unwrap()
            .with_behaviour(|key_pair| {
                let mut kad_config = kad::Config::new(
                    libp2p::StreamProtocol::new("/rust-proof/kad/1.0.0")
                );
                kad_config.set_periodic_bootstrap_interval(
                    Some(std::time::Duration::from_secs(10))
                );

                let gossipsub_config = gossipsub::ConfigBuilder
                    ::default()
                    .heartbeat_interval(std::time::Duration::from_secs(10))
                    .validation_mode(gossipsub::ValidationMode::Strict)
                    .message_id_fn(|message| {
                        use std::hash::{ Hash, Hasher };
                        let mut hasher = std::collections::hash_map::DefaultHasher::new();
                        message.data.hash(&mut hasher);
                        message.topic.hash(&mut hasher);
                        if let Some(peer_id) = message.source {
                            peer_id.hash(&mut hasher);
                        }
                        let now = std::time::SystemTime
                            ::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_millis();
                        now.to_string().hash(&mut hasher);
                        gossipsub::MessageId::from(hasher.finish().to_string())
                    })
                    .build()?;

                Ok(AppBehaviour {
                    kademlia: kad::Behaviour::with_config(
                        key_pair.public().to_peer_id(),
                        kad::store::MemoryStore::new(key_pair.public().to_peer_id()),
                        kad_config
                    ),
                    gossipsub: gossipsub::Behaviour::new(
                        gossipsub::MessageAuthenticity::Signed(key_pair.clone()),
                        gossipsub_config
                    )?,
                    request_response: request_response::json::Behaviour::new(
                        [
                            (
                                libp2p::StreamProtocol::new("/rust-proof/sync/1.0.0"),
                                request_response::ProtocolSupport::Full,
                            ),
                        ],
                        request_response::Config::default()
                    ),
                })
            })
            .unwrap()
            .with_swarm_config(|config| {
                config.with_idle_connection_timeout(std::time::Duration::from_secs(30))
            })
            .build();
        Self {
            node_sender,
            swarm,
        }
    }

    /// Starts the libp2p Swarm event loop.
    pub async fn start(&mut self) {
        // ====================================================================
        // TODO: Chapter 8 - Implement start
        // 1. Loop over `swarm.select_next_some().await`.
        // 2. Handle Gossipsub events (deserialize messages and send to Node).
        // 3. Handle RequestResponse events (fetch blocks and send back).
        // 4. Handle Kademlia events (add discovered peers to Gossipsub).
        // ====================================================================
        println!("NetworkManager started");
    }

    /// Broadcasts a transaction to the network via Gossipsub.
    pub async fn broadcast_transaction(&mut self) {
        // ====================================================================
        // TODO: Chapter 8 - Implement broadcast_transaction
        // 1. Serialize the transaction.
        // 2. Publish the serialized bytes to the "transactions" Gossipsub topic.
        // ====================================================================
        println!("Broadcasting transaction to network");
    }

    /// Broadcasts a block to the network via Gossipsub.
    pub async fn broadcast_block(&mut self) {
        // ====================================================================
        // TODO: Chapter 8 - Implement broadcast_block
        // 1. Serialize the block.
        // 2. Publish the serialized bytes to the "blocks" Gossipsub topic.
        // ====================================================================
        println!("Broadcasting block to network");
    }
}
