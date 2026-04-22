use libp2p::{
    futures::StreamExt,
    gossipsub,
    kad,
    noise,
    request_response,
    swarm::NetworkBehaviour,
    swarm::SwarmEvent,
    tcp,
    yamux,
    Swarm,
};
use rp_node::network::message::{ NetworkMessage, SyncRequest, SyncResponse };
use rp_node::contract::NodeAction;
use tokio::{ select, sync::mpsc };

#[derive(NetworkBehaviour)]
pub struct AppBehaviour {
    pub gossipsub: gossipsub::Behaviour,
    pub kademlia: kad::Behaviour<kad::store::MemoryStore>,
    pub request_response: request_response::json::Behaviour<SyncRequest, SyncResponse>,
}

/// Manages P2P network connections and message broadcasting using libp2p.
pub struct NetworkManager {
    swarm: Swarm<AppBehaviour>,
    node_sender: mpsc::Sender<NodeCommand>,
}

impl NetworkManager {
    pub fn new(node_sender: mpsc::Sender<NodeCommand>) -> Self {
        let swarm = libp2p::SwarmBuilder
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

    pub async fn start(&mut self) {
        loop {
            select! {
            event = self.swarm.select_next_some() => match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Listening on {}", address);
                }
                SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                    println!("Connected to {}", peer_id);
                }
                SwarmEvent::Behaviour(event) => {
                    match event {
                        AppBehaviourEvent::Gossipsub(gossipsub::Event::Message { message, .. }) => {
                            println!("Received Gossipsub message on topic {}: {:?}", message.topic, message.data);
                            if let Ok(network_message) = serde_json::from_slice::<NetworkMessage>(&message.data) {
                                println!("Deserialized NetworkMessage: {:?}", network_message);
                                match network_message {
                                    NetworkMessage::NewTransaction(tx) => {
                                        let _ = self.node_sender.send(NodeCommand::AddTransaction { transaction: tx, responder: tokio::sync::oneshot::channel().0 }).await;
                                    }
                                    NetworkMessage::NewBlock(block) => {
                                        let _ = self.node_sender.send(NodeCommand::AddBlock { block, responder: tokio::sync::oneshot::channel().0 }).await;
                                    }
                                }
                            } else {
                                println!("Failed to deserialize Gossipsub message");
                            }
                        }
                        AppBehaviourEvent::RequestResponse(event) => {
                            match event {
                                request_response::Event::Message { peer, connection_id, message } => {
                                    println!("Received RequestResponse message from {}, connection {}: {:?}", peer, connection_id, message);
                                    match message {
                                        request_response::Message::Request { request, channel, .. } => {
                                            println!("Received sync request: from {} to {}", request.from_height, request.to_height);
                                            let responder = tokio::sync::oneshot::channel();
                                            self.node_sender.send(NodeCommand::GetBlocksByHeight {
                                                from_height: request.from_height,
                                                to_height: request.to_height,
                                                responder: responder.0,
                                            }).await.unwrap();
                                            let blocks = responder.1.await.unwrap().unwrap();
                                            println!("Fetched {} blocks from Node", blocks.len());
                                            self.swarm.behaviour_mut().request_response.send_response(channel, SyncResponse { blocks }).unwrap();
                                        }
                                        request_response::Message::Response { response, .. } => {
                                            println!("Received sync response with {} blocks", response.blocks.len());
                                            for block in &response.blocks {
                                                let _ = self.node_sender.send(NodeCommand::AddBlock { block: block.clone(), responder: tokio::sync::oneshot::channel().0 }).await;
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        AppBehaviourEvent::Kademlia(event) => {
                            println!("Received Kademlia event: {:?}", event);
                        }
                        _ => {
                            println!("Received other event: {:?}", event);
                        }
                    }
                }
                _ => {}
            },
            _ = tokio::signal::ctrl_c() => {
                println!("Shutting down...");
                break;
                }
            }
        }
    }

    pub async fn broadcast_transaction(&mut self) {
        println!("Broadcasting transaction to network");
    }

    pub async fn broadcast_block(&mut self) {
        println!("Broadcasting block to network");
    }
}
