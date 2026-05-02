use std::{
    collections::{ HashMap, HashSet, VecDeque },
    fs,
    io::{ self, ErrorKind },
    path::Path,
    time::Duration,
};

use libp2p::{
    core::ConnectedPoint,
    futures::{ AsyncRead, AsyncWrite, StreamExt },
    gossipsub,
    identify,
    identity,
    kad,
    noise,
    request_response::{ self, Codec, OutboundRequestId, ProtocolSupport, ResponseChannel },
    swarm::{ NetworkBehaviour, SwarmEvent },
    tcp,
    yamux,
    PeerId,
    StreamProtocol,
    Swarm,
    SwarmBuilder,
};
use log::{ debug, info, warn };
use rand::rngs::OsRng;
use rp_core::{
    crypto::{
        Signature,
        Signer,
        SigningKey,
        Verifier,
        signing_key_from_bytes,
        verifying_key_bytes,
        verifying_key_from_bytes,
    },
    traits::{ FromBytes, Hashable, ToBytes },
};
use rp_node::{
    contract::PeerId as NodePeerId,
    network::message::{
        AnnounceRequest,
        AnnounceResponse,
        NetworkMessage,
        SyncRequest,
        SyncResponse,
    },
};
use serde::{ Deserialize, Serialize };

use crate::{
    network::{
        codec::postcard::{ read_postcard_frame, write_postcard_frame },
        config::{ DEFAULT_MAX_FRAME_LEN, NetworkConfig },
    },
    runtime::{
        errors::RuntimeError,
        manager::{ EventTx, NetworkCommand, NetworkRx, RuntimeEvent },
    },
};

const TAG: &str = "network";
const GOSSIPSUB_ANNOUNCE_TOPIC: &str = "rust-proof/announce";
const NODE_IDENTITY_FILE: &str = "node_identity.bin";
const TRANSPORT_IDENTITY_FILE: &str = "transport_identity.bin";
const IDENTIFY_PROTOCOL_VERSION: &str = "rust-proof/1";
const IDENTIFY_AGENT_VERSION: &str = "rp-runtime/0.1.0";
const NODE_HELLO_PROTOCOL: &str = "/rust-proof/node-hello/1";
const SYNC_PROTOCOL: &str = "/rust-proof/sync/1";
const ANNOUNCE_PROTOCOL: &str = "/rust-proof/announce/1";

struct HostNodeIdentity {
    signing_key: SigningKey,
    public_key: Vec<u8>,
    peer_id: NodePeerId,
}

impl HostNodeIdentity {
    fn load_or_create(path: &Path) -> Result<Self, RuntimeError> {
        if path.exists() {
            let bytes = fs::read(path).map_err(RuntimeError::io_other)?;
            let private_key: [u8; 32] = bytes
                .try_into()
                .map_err(|_| RuntimeError::config("invalid persisted node identity bytes"))?;
            let signing_key = signing_key_from_bytes(&private_key).map_err(RuntimeError::crypto)?;
            return Ok(Self::from_signing_key(signing_key));
        }

        let signing_key = SigningKey::random(&mut OsRng);
        let bytes = signing_key.to_bytes();
        fs::write(path, bytes.as_slice()).map_err(RuntimeError::io_other)?;
        Ok(Self::from_signing_key(signing_key))
    }

    fn from_signing_key(signing_key: SigningKey) -> Self {
        let public_key = verifying_key_bytes(signing_key.verifying_key());
        let peer_id = public_key.hash();

        Self {
            signing_key,
            public_key,
            peer_id,
        }
    }

    fn build_signature(&self, message: &[u8]) -> Vec<u8> {
        let signature: Signature = self.signing_key.sign(message);
        signature.to_bytes().to_vec()
    }
}

struct HostTransportIdentity {
    peer_id_bytes: Vec<u8>,
}

impl HostTransportIdentity {
    fn load_or_create(path: &Path) -> Result<(Self, identity::Keypair), RuntimeError> {
        let keypair = if path.exists() {
            let bytes = fs::read(path).map_err(RuntimeError::io_other)?;
            identity::Keypair
                ::from_protobuf_encoding(&bytes)
                .map_err(|_| RuntimeError::crypto("invalid persisted transport identity bytes"))?
        } else {
            let keypair = identity::Keypair::generate_ed25519();
            let encoded = keypair
                .to_protobuf_encoding()
                .map_err(|_| RuntimeError::crypto("failed to encode transport identity keypair"))?;
            fs::write(path, &encoded).map_err(RuntimeError::io_other)?;
            keypair
        };

        let public_key = keypair.public();
        let identity = Self {
            peer_id_bytes: public_key.to_peer_id().to_bytes(),
        };

        Ok((identity, keypair))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NodeHello {
    version: u16,
    node_public_key: Vec<u8>,
    node_peer_id: NodePeerId,
    transport_peer_id: Vec<u8>,
    max_frame_len: u32,
    max_blocks_per_chunk: u16,
    capabilities: PeerCapabilities,
    signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NodeHelloResponse {
    accepted: bool,
    remote: NodeHello,
    reject_reason: Option<NodeHelloRejectReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum NodeHelloRejectReason {
    VersionMismatch,
    InvalidSignature,
    PeerIdMismatch,
    TransportBindingMismatch,
    UnsupportedRequiredProtocol,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PeerCapabilities {
    supports_sync_v1: bool,
    supports_announce_v1: bool,
    supports_ping: bool,
}

#[derive(Debug, Clone, Serialize)]
struct NodeHelloTranscript<'a> {
    version: u16,
    node_peer_id: NodePeerId,
    transport_peer_id: &'a [u8],
    max_frame_len: u32,
    max_blocks_per_chunk: u16,
    capabilities: &'a PeerCapabilities,
}

#[derive(Debug, Clone)]
struct VerifiedPeer {
    node_peer_id: NodePeerId,
    max_blocks_per_chunk: u16,
}

struct NodeHelloVerifier;

impl NodeHelloVerifier {
    fn verify(
        remote: &NodeHello,
        authenticated_transport_peer: &[u8]
    ) -> Result<VerifiedPeer, RuntimeError> {
        if remote.transport_peer_id.as_slice() != authenticated_transport_peer {
            return Err(
                RuntimeError::config(
                    "node hello transport peer id does not match authenticated session peer"
                )
            );
        }

        let derived_node_peer_id = remote.node_public_key.hash();
        if derived_node_peer_id != remote.node_peer_id {
            return Err(RuntimeError::config("node hello peer id does not match node public key"));
        }

        let verifying_key_bytes = remote.node_public_key
            .as_slice()
            .try_into()
            .map_err(|_| {
                RuntimeError::config("node hello public key must be a compressed 33-byte P-256 key")
            })?;
        let verifying_key = verifying_key_from_bytes(&verifying_key_bytes).map_err(
            RuntimeError::crypto
        )?;

        let transcript = NodeHelloTranscript {
            version: remote.version,
            node_peer_id: remote.node_peer_id,
            transport_peer_id: &remote.transport_peer_id,
            max_frame_len: remote.max_frame_len,
            max_blocks_per_chunk: remote.max_blocks_per_chunk,
            capabilities: &remote.capabilities,
        };
        let transcript_bytes = postcard
            ::to_allocvec(&transcript)
            .map_err(|_| RuntimeError::crypto("failed to serialize node hello transcript"))?;
        let signature = Signature::from_slice(&remote.signature).map_err(|_|
            RuntimeError::crypto("invalid node hello signature bytes")
        )?;

        verifying_key
            .verify(&transcript_bytes, &signature)
            .map_err(|_| RuntimeError::config("node hello signature verification failed"))?;

        Ok(VerifiedPeer {
            node_peer_id: remote.node_peer_id,
            max_blocks_per_chunk: remote.max_blocks_per_chunk,
        })
    }
}

fn node_hello_reject_reason(error: &RuntimeError) -> NodeHelloRejectReason {
    match error {
        RuntimeError::Config(message) => match *message {
            "node hello transport peer id does not match authenticated session peer" => {
                NodeHelloRejectReason::TransportBindingMismatch
            }
            "node hello peer id does not match node public key" => {
                NodeHelloRejectReason::PeerIdMismatch
            }
            _ => NodeHelloRejectReason::InvalidSignature,
        },
        RuntimeError::Crypto(_) => NodeHelloRejectReason::InvalidSignature,
        _ => NodeHelloRejectReason::InvalidSignature,
    }
}

#[derive(Clone, Default)]
struct NodeHelloCodec;

#[derive(Clone, Default)]
struct SyncCodec;

#[derive(Clone, Default)]
struct AnnounceCodec;

#[derive(NetworkBehaviour)]
struct RuntimeBehaviour {
    gossipsub: gossipsub::Behaviour,
    kademlia: kad::Behaviour<kad::store::MemoryStore>,
    identify: identify::Behaviour,
    node_hello: request_response::Behaviour<NodeHelloCodec>,
    sync: request_response::Behaviour<SyncCodec>,
    announce: request_response::Behaviour<AnnounceCodec>,
}

pub struct NetworkManager {
    config: NetworkConfig,
    node_identity: HostNodeIdentity,
    transport_identity: HostTransportIdentity,
    event_tx: EventTx,
    network_rx: NetworkRx,
    swarm: Swarm<RuntimeBehaviour>,
    announce_topic: gossipsub::IdentTopic,
    peer_by_node: HashMap<NodePeerId, PeerId>,
    node_by_transport: HashMap<PeerId, NodePeerId>,
    peer_max_blocks_per_chunk: HashMap<NodePeerId, u16>,
    host_transport_peers: HashSet<PeerId>,
    pending_node_hello: HashSet<PeerId>,
    pending_sync_responses: HashMap<NodePeerId, VecDeque<ResponseChannel<SyncResponse>>>,
    outbound_sync_requests: HashMap<OutboundRequestId, NodePeerId>,
}

impl NetworkManager {
    pub fn new(
        network_rx: NetworkRx,
        event_tx: EventTx,
        data_dir: impl AsRef<Path>
    ) -> Result<Self, RuntimeError> {
        let config = NetworkConfig::from_env()?;
        Self::new_with_config(network_rx, event_tx, data_dir, config)
    }

    fn new_with_config(
        network_rx: NetworkRx,
        event_tx: EventTx,
        data_dir: impl AsRef<Path>,
        config: NetworkConfig
    ) -> Result<Self, RuntimeError> {
        let data_dir = data_dir.as_ref();
        fs::create_dir_all(data_dir).map_err(RuntimeError::io_other)?;

        let node_identity = HostNodeIdentity::load_or_create(&data_dir.join(NODE_IDENTITY_FILE))?;
        let (transport_identity, transport_keypair) = HostTransportIdentity::load_or_create(
            &data_dir.join(TRANSPORT_IDENTITY_FILE)
        )?;
        let mut swarm = build_swarm(transport_keypair, &config)?;
        let announce_topic = gossipsub::IdentTopic::new(GOSSIPSUB_ANNOUNCE_TOPIC);
        swarm.behaviour_mut().gossipsub.subscribe(&announce_topic).map_err(RuntimeError::io_other)?;

        Ok(Self {
            config,
            node_identity,
            transport_identity,
            event_tx,
            network_rx,
            swarm,
            announce_topic,
            peer_by_node: HashMap::new(),
            node_by_transport: HashMap::new(),
            peer_max_blocks_per_chunk: HashMap::new(),
            host_transport_peers: HashSet::new(),
            pending_node_hello: HashSet::new(),
            pending_sync_responses: HashMap::new(),
            outbound_sync_requests: HashMap::new(),
        })
    }

    pub async fn run(&mut self) -> Result<(), RuntimeError> {
        for address in self.config.bootstrap_addrs.clone() {
            if let Err(error) = self.swarm.dial(address.clone()) {
                warn!(target: TAG, "failed to dial bootstrap peer {address}: {error}");
            }
        }

        loop {
            tokio::select! {
                maybe_command = self.network_rx.recv() => {
                    let Some(command) = maybe_command else {
                        return Ok(());
                    };

                    self.handle_command(command)?;
                }
                swarm_event = self.swarm.select_next_some() => {
                    self.handle_swarm_event(swarm_event).await?;
                }
            }
        }
    }

    fn handle_command(&mut self, command: NetworkCommand) -> Result<(), RuntimeError> {
        match command {
            NetworkCommand::SendFrame { peer, frame } => self.send_frame(peer, frame),
            NetworkCommand::BroadcastFrame { frame } => {
                if
                    matches!(
                        NetworkMessage::from_bytes(&frame),
                        Ok(NetworkMessage::AnnounceRequest(_))
                    )
                {
                    if
                        let Err(error) = self.swarm
                            .behaviour_mut()
                            .gossipsub.publish(self.announce_topic.clone(), frame.clone())
                    {
                        warn!(target: TAG, "failed to publish announce frame over gossipsub: {error}");
                    }
                }

                let peers = self.peer_by_node.keys().copied().collect::<Vec<_>>();
                for peer in peers {
                    if
                        !self.peer_by_node
                            .get(&peer)
                            .map(|transport_peer|
                                self.host_transport_peers.contains(transport_peer)
                            )
                            .unwrap_or(false)
                    {
                        self.send_frame(peer, frame.clone())?;
                    }
                }
                Ok(())
            }
            NetworkCommand::DisconnectPeer { peer } => {
                if let Some(transport_peer) = self.peer_by_node.get(&peer).copied() {
                    let _ = self.swarm.disconnect_peer_id(transport_peer);
                }
                Ok(())
            }
            NetworkCommand::RequestBlocks { peer, from_height, to_height } =>
                self.send_sync_request(peer, SyncRequest {
                    from_height,
                    to_height,
                }),
        }
    }

    fn send_frame(&mut self, peer: NodePeerId, frame: Vec<u8>) -> Result<(), RuntimeError> {
        let Some(transport_peer) = self.peer_by_node.get(&peer).copied() else {
            warn!(target: TAG, "dropping outbound frame for unknown peer: {:?}", peer);
            return Ok(());
        };

        let message = NetworkMessage::from_bytes(&frame).map_err(|_|
            RuntimeError::config("invalid outbound network frame")
        )?;

        match message {
            NetworkMessage::AnnounceRequest(request) => {
                self.swarm.behaviour_mut().announce.send_request(&transport_peer, request);
                Ok(())
            }
            NetworkMessage::SyncRequest(request) => self.send_sync_request(peer, request),
            NetworkMessage::SyncResponse(response) => self.send_sync_response(peer, response),
            NetworkMessage::AnnounceResponse(_) => {
                warn!(target: TAG, "unexpected outbound announce response for peer {:?}", peer);
                Ok(())
            }
        }
    }

    fn send_sync_request(
        &mut self,
        peer: NodePeerId,
        request: SyncRequest
    ) -> Result<(), RuntimeError> {
        let Some(transport_peer) = self.peer_by_node.get(&peer).copied() else {
            warn!(target: TAG, "dropping sync request for unknown peer: {:?}", peer);
            return Ok(());
        };

        let request = self.clamp_sync_request(peer, request);
        let request_id = self.swarm.behaviour_mut().sync.send_request(&transport_peer, request);
        self.outbound_sync_requests.insert(request_id, peer);
        Ok(())
    }

    fn clamp_sync_request(&self, peer: NodePeerId, request: SyncRequest) -> SyncRequest {
        let Some(limit) = self.peer_max_blocks_per_chunk.get(&peer).copied() else {
            return request;
        };
        if limit == 0 || request.from_height > request.to_height {
            return request;
        }

        let max_to_height = request.from_height.saturating_add((limit as u64).saturating_sub(1));
        SyncRequest {
            from_height: request.from_height,
            to_height: request.to_height.min(max_to_height),
        }
    }

    fn send_sync_response(
        &mut self,
        peer: NodePeerId,
        response: SyncResponse
    ) -> Result<(), RuntimeError> {
        let Some(queue) = self.pending_sync_responses.get_mut(&peer) else {
            warn!(target: TAG, "no pending sync response channel for peer {:?}", peer);
            return Ok(());
        };

        let Some(channel) = queue.pop_front() else {
            warn!(target: TAG, "empty sync response queue for peer {:?}", peer);
            return Ok(());
        };
        let should_remove = queue.is_empty();
        if should_remove {
            self.pending_sync_responses.remove(&peer);
        }

        self.swarm
            .behaviour_mut()
            .sync.send_response(channel, response)
            .map_err(|_| RuntimeError::io_other("failed to send sync response"))
    }

    async fn handle_swarm_event(
        &mut self,
        event: SwarmEvent<RuntimeBehaviourEvent>
    ) -> Result<(), RuntimeError> {
        match event {
            SwarmEvent::NewListenAddr { address, .. } => {
                info!(target: TAG, "listening on {address}");
            }
            SwarmEvent::IncomingConnection { local_addr, send_back_addr, .. } => {
                debug!(target: TAG, "incoming tcp connection on {local_addr} from {send_back_addr}");
            }
            SwarmEvent::IncomingConnectionError { local_addr, send_back_addr, error, .. } => {
                warn!(target: TAG, "incoming connection error on {local_addr} from {send_back_addr}: {error}");
            }
            SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                debug!(target: TAG, "connection established with {peer_id}");
                if
                    let Some(address) = match &endpoint {
                        ConnectedPoint::Dialer { address, .. } => Some(address.clone()),
                        ConnectedPoint::Listener { send_back_addr, .. } =>
                            Some(send_back_addr.clone()),
                    } 
                {
                    self.swarm.behaviour_mut().kademlia.add_address(&peer_id, address);
                }
                if
                    matches!(endpoint, ConnectedPoint::Dialer { .. }) &&
                    !self.node_by_transport.contains_key(&peer_id)
                {
                    self.pending_node_hello.insert(peer_id);
                }
            }
            SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                debug!(target: TAG, "connection closed with {peer_id}: {:?}", cause);
                self.unregister_transport_peer(peer_id).await?;
            }
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                warn!(target: TAG, "outgoing connection error for {:?}: {error}", peer_id);
            }
            SwarmEvent::Behaviour(RuntimeBehaviourEvent::Identify(event)) => {
                self.handle_identify_event(event)?;
            }
            SwarmEvent::Behaviour(RuntimeBehaviourEvent::Gossipsub(event)) => {
                self.handle_gossipsub_event(event).await?;
            }
            SwarmEvent::Behaviour(RuntimeBehaviourEvent::Kademlia(event)) => {
                debug!(target: TAG, "kademlia event: {:?}", event);
            }
            SwarmEvent::Behaviour(RuntimeBehaviourEvent::NodeHello(event)) => {
                self.handle_node_hello_event(event).await?;
            }
            SwarmEvent::Behaviour(RuntimeBehaviourEvent::Sync(event)) => {
                self.handle_sync_event(event).await?;
            }
            SwarmEvent::Behaviour(RuntimeBehaviourEvent::Announce(event)) => {
                self.handle_announce_event(event).await?;
            }
            other => {
                debug!(target: TAG, "swarm event: {:?}", other);
            }
        }

        Ok(())
    }

    fn handle_identify_event(&mut self, event: identify::Event) -> Result<(), RuntimeError> {
        match event {
            identify::Event::Received { peer_id, info, .. } => {
                for address in info.listen_addrs {
                    self.swarm.behaviour_mut().kademlia.add_address(&peer_id, address);
                }

                if info.agent_version.starts_with("rp-runtime/") {
                    self.host_transport_peers.insert(peer_id);
                } else {
                    self.host_transport_peers.remove(&peer_id);
                }

                if
                    self.pending_node_hello.remove(&peer_id) &&
                    !self.node_by_transport.contains_key(&peer_id)
                {
                    let Some(hello) = self.build_local_node_hello_or_disconnect(
                        peer_id,
                        "failed to build outbound node hello"
                    ) else {
                        return Ok(());
                    };
                    self.swarm.behaviour_mut().node_hello.send_request(&peer_id, hello);
                }

                debug!(target: TAG, "identify received from {peer_id}: agent={}", info.agent_version);
            }
            identify::Event::Error { peer_id, error, .. } => {
                warn!(target: TAG, "identify error from {peer_id}: {error}");
            }
            other => {
                debug!(target: TAG, "identify event: {:?}", other);
            }
        }

        Ok(())
    }

    async fn handle_gossipsub_event(
        &mut self,
        event: gossipsub::Event
    ) -> Result<(), RuntimeError> {
        match event {
            gossipsub::Event::Message { propagation_source, message, .. } => {
                let Some(node_peer) = self.node_by_transport
                    .get(&propagation_source)
                    .copied() else {
                    debug!(target: TAG, "ignoring gossipsub message from unknown transport peer {propagation_source}");
                    return Ok(());
                };

                self.event_tx
                    .send(RuntimeEvent::FrameReceived {
                        peer: node_peer,
                        frame: message.data,
                    }).await
                    .map_err(RuntimeError::event_send)?;
            }
            other => {
                debug!(target: TAG, "gossipsub event: {:?}", other);
            }
        }

        Ok(())
    }

    async fn handle_node_hello_event(
        &mut self,
        event: request_response::Event<NodeHello, NodeHelloResponse>
    ) -> Result<(), RuntimeError> {
        match event {
            request_response::Event::Message { peer, message, .. } =>
                match message {
                    request_response::Message::Request { request, channel, .. } => {
                        let verified = match NodeHelloVerifier::verify(&request, &peer.to_bytes()) {
                            Ok(verified) => verified,
                            Err(error) => {
                                let reject_reason = node_hello_reject_reason(&error);
                                warn!(
                                    target: TAG,
                                    "rejecting invalid node hello request from {peer}: {:?} ({:?})",
                                    reject_reason,
                                    error
                                );

                                let Some(remote) = self.build_local_node_hello_or_disconnect(
                                    peer,
                                    "failed to build node hello rejection"
                                ) else {
                                    return Ok(());
                                };

                                let response = NodeHelloResponse {
                                    accepted: false,
                                    remote,
                                    reject_reason: Some(reject_reason),
                                };

                                if self
                                    .swarm
                                    .behaviour_mut()
                                    .node_hello
                                    .send_response(channel, response)
                                    .is_err()
                                {
                                    warn!(target: TAG, "failed to send node hello rejection to {peer}");
                                }

                                let _ = self.swarm.disconnect_peer_id(peer);
                                return Ok(());
                            }
                        };
                        let Some(remote) = self.build_local_node_hello_or_disconnect(
                            peer,
                            "failed to build node hello response"
                        ) else {
                            return Ok(());
                        };
                        let response = NodeHelloResponse {
                            accepted: true,
                            remote,
                            reject_reason: None,
                        };
                        if self
                            .swarm
                            .behaviour_mut()
                            .node_hello
                            .send_response(channel, response)
                            .is_err()
                        {
                            warn!(target: TAG, "failed to send node hello response to {peer}");
                            let _ = self.swarm.disconnect_peer_id(peer);
                            return Ok(());
                        }
                        self.register_verified_peer(peer, verified).await?;
                    }
                    request_response::Message::Response { response, .. } => {
                        if !response.accepted {
                            warn!(target: TAG, "peer {peer} rejected node hello: {:?}", response.reject_reason);
                            let _ = self.swarm.disconnect_peer_id(peer);
                            return Ok(());
                        }

                        let verified = match NodeHelloVerifier::verify(
                            &response.remote,
                            &peer.to_bytes()
                        ) {
                            Ok(verified) => verified,
                            Err(error) => {
                                warn!(target: TAG, "disconnecting peer {peer} after invalid node hello response: {:?}", error);
                                let _ = self.swarm.disconnect_peer_id(peer);
                                return Ok(());
                            }
                        };
                        self.register_verified_peer(peer, verified).await?;
                    }
                }
            request_response::Event::OutboundFailure { peer, request_id, error, .. } => {
                warn!(target: TAG, "node hello outbound failure for {peer} request {request_id}: {error}");
            }
            request_response::Event::InboundFailure { peer, request_id, error, .. } => {
                warn!(target: TAG, "node hello inbound failure for {peer} request {request_id}: {error}");
            }
            request_response::Event::ResponseSent { peer, request_id, .. } => {
                debug!(target: TAG, "node hello response sent to {peer} for {request_id}");
            }
        }

        Ok(())
    }

    fn build_local_node_hello_or_disconnect(
        &mut self,
        peer: PeerId,
        context: &'static str
    ) -> Option<NodeHello> {
        match self.build_local_node_hello() {
            Ok(hello) => Some(hello),
            Err(error) => {
                warn!(target: TAG, "{context} for {peer}: {:?}", error);
                let _ = self.swarm.disconnect_peer_id(peer);
                None
            }
        }
    }

    async fn handle_sync_event(
        &mut self,
        event: request_response::Event<SyncRequest, SyncResponse>
    ) -> Result<(), RuntimeError> {
        match event {
            request_response::Event::Message { peer, message, .. } =>
                match message {
                    request_response::Message::Request { request, channel, .. } => {
                        let Some(node_peer) = self.node_by_transport.get(&peer).copied() else {
                            warn!(target: TAG, "sync request arrived before node hello completed for {peer}");
                            let empty = SyncResponse {
                                blocks: Vec::new(),
                                has_more: false,
                                next_height: None,
                            };
                            self.swarm
                                .behaviour_mut()
                                .sync.send_response(channel, empty)
                                .map_err(|_|
                                    RuntimeError::io_other("failed to send fallback sync response")
                                )?;
                            return Ok(());
                        };

                        self.pending_sync_responses
                            .entry(node_peer)
                            .or_default()
                            .push_back(channel);
                        self.event_tx
                            .send(RuntimeEvent::FrameReceived {
                                peer: node_peer,
                                frame: NetworkMessage::SyncRequest(request).to_bytes(),
                            }).await
                            .map_err(RuntimeError::event_send)?;
                    }
                    request_response::Message::Response { request_id, response } => {
                        let Some(node_peer) = self.outbound_sync_requests.remove(&request_id) else {
                            warn!(target: TAG, "sync response for unknown request {request_id} from {peer}");
                            return Ok(());
                        };

                        self.event_tx
                            .send(RuntimeEvent::FrameReceived {
                                peer: node_peer,
                                frame: NetworkMessage::SyncResponse(response).to_bytes(),
                            }).await
                            .map_err(RuntimeError::event_send)?;
                    }
                }
            request_response::Event::OutboundFailure { peer, request_id, error, .. } => {
                self.outbound_sync_requests.remove(&request_id);
                warn!(target: TAG, "sync outbound failure for {peer} request {request_id}: {error}");
            }
            request_response::Event::InboundFailure { peer, request_id, error, .. } => {
                warn!(target: TAG, "sync inbound failure for {peer} request {request_id}: {error}");
            }
            request_response::Event::ResponseSent { peer, request_id, .. } => {
                debug!(target: TAG, "sync response sent to {peer} for {request_id}");
            }
        }

        Ok(())
    }

    async fn handle_announce_event(
        &mut self,
        event: request_response::Event<AnnounceRequest, AnnounceResponse>
    ) -> Result<(), RuntimeError> {
        match event {
            request_response::Event::Message { peer, message, .. } =>
                match message {
                    request_response::Message::Request { request, channel, .. } => {
                        let accepted = if
                            let Some(node_peer) = self.node_by_transport.get(&peer).copied()
                        {
                            self.event_tx
                                .send(RuntimeEvent::FrameReceived {
                                    peer: node_peer,
                                    frame: NetworkMessage::AnnounceRequest(request).to_bytes(),
                                }).await
                                .map_err(RuntimeError::event_send)?;
                            true
                        } else {
                            warn!(target: TAG, "announce request arrived before node hello completed for {peer}");
                            false
                        };

                        self.swarm
                            .behaviour_mut()
                            .announce.send_response(channel, AnnounceResponse { accepted })
                            .map_err(|_|
                                RuntimeError::io_other("failed to send announce response")
                            )?;
                    }
                    request_response::Message::Response { response, .. } => {
                        let Some(node_peer) = self.node_by_transport.get(&peer).copied() else {
                            warn!(target: TAG, "announce response arrived before node hello completed for {peer}");
                            return Ok(());
                        };

                        self.event_tx
                            .send(RuntimeEvent::FrameReceived {
                                peer: node_peer,
                                frame: NetworkMessage::AnnounceResponse(response).to_bytes(),
                            }).await
                            .map_err(RuntimeError::event_send)?;
                    }
                }
            request_response::Event::OutboundFailure { peer, request_id, error, .. } => {
                warn!(target: TAG, "announce outbound failure for {peer} request {request_id}: {error}");
            }
            request_response::Event::InboundFailure { peer, request_id, error, .. } => {
                warn!(target: TAG, "announce inbound failure for {peer} request {request_id}: {error}");
            }
            request_response::Event::ResponseSent { peer, request_id, .. } => {
                debug!(target: TAG, "announce response sent to {peer} for {request_id}");
            }
        }

        Ok(())
    }

    async fn register_verified_peer(
        &mut self,
        transport_peer: PeerId,
        verified: VerifiedPeer
    ) -> Result<(), RuntimeError> {
        let node_peer = verified.node_peer_id;
        let previous_transport = self.peer_by_node.insert(node_peer, transport_peer);
        let previous_node = self.node_by_transport.insert(transport_peer, node_peer);
        self.peer_max_blocks_per_chunk.insert(node_peer, verified.max_blocks_per_chunk);

        if let Some(previous_transport) = previous_transport {
            if previous_transport != transport_peer {
                self.node_by_transport.remove(&previous_transport);
            }
        }
        if let Some(previous_node) = previous_node {
            if previous_node != node_peer {
                self.peer_by_node.remove(&previous_node);
            }
        }

        let is_new_mapping =
            previous_transport != Some(transport_peer) || previous_node != Some(node_peer);
        if is_new_mapping {
            info!(target: TAG, "registered node peer {:?} on transport peer {transport_peer}", node_peer);
            self.event_tx
                .send(RuntimeEvent::PeerConnected { peer: node_peer }).await
                .map_err(RuntimeError::event_send)?;
        }

        Ok(())
    }

    async fn unregister_transport_peer(
        &mut self,
        transport_peer: PeerId
    ) -> Result<(), RuntimeError> {
        let Some(node_peer) = self.node_by_transport.remove(&transport_peer) else {
            return Ok(());
        };

        self.peer_by_node.remove(&node_peer);
        self.peer_max_blocks_per_chunk.remove(&node_peer);
        self.host_transport_peers.remove(&transport_peer);
        self.pending_node_hello.remove(&transport_peer);
        self.pending_sync_responses.remove(&node_peer);
        self.outbound_sync_requests.retain(|_, pending_peer| *pending_peer != node_peer);

        self.event_tx
            .send(RuntimeEvent::PeerDisconnected { peer: node_peer }).await
            .map_err(RuntimeError::event_send)?;

        Ok(())
    }

    fn build_local_node_hello(&self) -> Result<NodeHello, RuntimeError> {
        let capabilities = PeerCapabilities {
            supports_sync_v1: true,
            supports_announce_v1: true,
            supports_ping: false,
        };
        let transcript = NodeHelloTranscript {
            version: 1,
            node_peer_id: self.node_identity.peer_id,
            transport_peer_id: &self.transport_identity.peer_id_bytes,
            max_frame_len: self.config.max_frame_len,
            max_blocks_per_chunk: self.config.max_blocks_per_chunk,
            capabilities: &capabilities,
        };
        let transcript_bytes = postcard
            ::to_allocvec(&transcript)
            .map_err(|_| RuntimeError::crypto("failed to serialize node hello transcript"))?;

        Ok(NodeHello {
            version: transcript.version,
            node_public_key: self.node_identity.public_key.clone(),
            node_peer_id: self.node_identity.peer_id,
            transport_peer_id: self.transport_identity.peer_id_bytes.clone(),
            max_frame_len: self.config.max_frame_len,
            max_blocks_per_chunk: self.config.max_blocks_per_chunk,
            capabilities,
            signature: self.node_identity.build_signature(&transcript_bytes),
        })
    }
}

fn build_swarm(
    transport_keypair: identity::Keypair,
    config: &NetworkConfig
) -> Result<Swarm<RuntimeBehaviour>, RuntimeError> {
    let mut swarm = SwarmBuilder::with_existing_identity(transport_keypair)
        .with_tokio()
        .with_tcp(tcp::Config::default(), noise::Config::new, yamux::Config::default)
        .map_err(RuntimeError::io_other)?
        .with_behaviour(move |local_key| {
            let local_peer_id = local_key.public().to_peer_id();
            let identify_config = identify::Config
                ::new(IDENTIFY_PROTOCOL_VERSION.to_string(), local_key.public())
                .with_agent_version(IDENTIFY_AGENT_VERSION.to_string());

            let gossipsub_config = gossipsub::ConfigBuilder
                ::default()
                .heartbeat_interval(Duration::from_secs(10))
                .validation_mode(gossipsub::ValidationMode::Strict)
                .build()
                .map_err(|message| io::Error::new(ErrorKind::InvalidInput, message))?;

            Ok(RuntimeBehaviour {
                gossipsub: gossipsub::Behaviour
                    ::new(
                        gossipsub::MessageAuthenticity::Signed(local_key.clone()),
                        gossipsub_config
                    )
                    .map_err(|message| io::Error::new(ErrorKind::InvalidInput, message))?,
                kademlia: kad::Behaviour::new(
                    local_peer_id,
                    kad::store::MemoryStore::new(local_peer_id)
                ),
                identify: identify::Behaviour::new(identify_config),
                node_hello: request_response::Behaviour::new(
                    [(StreamProtocol::new(NODE_HELLO_PROTOCOL), ProtocolSupport::Full)],
                    request_response::Config::default().with_request_timeout(config.request_timeout)
                ),
                sync: request_response::Behaviour::new(
                    [(StreamProtocol::new(SYNC_PROTOCOL), ProtocolSupport::Full)],
                    request_response::Config::default().with_request_timeout(config.request_timeout)
                ),
                announce: request_response::Behaviour::new(
                    [(StreamProtocol::new(ANNOUNCE_PROTOCOL), ProtocolSupport::Full)],
                    request_response::Config::default().with_request_timeout(config.request_timeout)
                ),
            })
        })
        .map_err(RuntimeError::io_other)?
        .with_swarm_config(|cfg| { cfg.with_idle_connection_timeout(config.idle_timeout) })
        .build();

    swarm.listen_on(config.listen_addr.clone()).map_err(RuntimeError::io_other)?;

    Ok(swarm)
}

#[async_trait::async_trait]
impl Codec for NodeHelloCodec {
    type Protocol = StreamProtocol;
    type Request = NodeHello;
    type Response = NodeHelloResponse;

    async fn read_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T
    ) -> io::Result<Self::Request>
        where T: AsyncRead + Unpin + Send
    {
        read_postcard_frame(io, DEFAULT_MAX_FRAME_LEN).await
    }

    async fn read_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T
    ) -> io::Result<Self::Response>
        where T: AsyncRead + Unpin + Send
    {
        read_postcard_frame(io, DEFAULT_MAX_FRAME_LEN).await
    }

    async fn write_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        request: Self::Request
    ) -> io::Result<()>
        where T: AsyncWrite + Unpin + Send
    {
        write_postcard_frame(io, &request, DEFAULT_MAX_FRAME_LEN).await
    }

    async fn write_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        response: Self::Response
    ) -> io::Result<()>
        where T: AsyncWrite + Unpin + Send
    {
        write_postcard_frame(io, &response, DEFAULT_MAX_FRAME_LEN).await
    }
}

#[async_trait::async_trait]
impl Codec for SyncCodec {
    type Protocol = StreamProtocol;
    type Request = SyncRequest;
    type Response = SyncResponse;

    async fn read_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T
    ) -> io::Result<Self::Request>
        where T: AsyncRead + Unpin + Send
    {
        read_postcard_frame(io, DEFAULT_MAX_FRAME_LEN).await
    }

    async fn read_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T
    ) -> io::Result<Self::Response>
        where T: AsyncRead + Unpin + Send
    {
        read_postcard_frame(io, DEFAULT_MAX_FRAME_LEN).await
    }

    async fn write_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        request: Self::Request
    ) -> io::Result<()>
        where T: AsyncWrite + Unpin + Send
    {
        write_postcard_frame(io, &request, DEFAULT_MAX_FRAME_LEN).await
    }

    async fn write_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        response: Self::Response
    ) -> io::Result<()>
        where T: AsyncWrite + Unpin + Send
    {
        write_postcard_frame(io, &response, DEFAULT_MAX_FRAME_LEN).await
    }
}

#[async_trait::async_trait]
impl Codec for AnnounceCodec {
    type Protocol = StreamProtocol;
    type Request = AnnounceRequest;
    type Response = AnnounceResponse;

    async fn read_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T
    ) -> io::Result<Self::Request>
        where T: AsyncRead + Unpin + Send
    {
        read_postcard_frame(io, DEFAULT_MAX_FRAME_LEN).await
    }

    async fn read_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T
    ) -> io::Result<Self::Response>
        where T: AsyncRead + Unpin + Send
    {
        read_postcard_frame(io, DEFAULT_MAX_FRAME_LEN).await
    }

    async fn write_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        request: Self::Request
    ) -> io::Result<()>
        where T: AsyncWrite + Unpin + Send
    {
        write_postcard_frame(io, &request, DEFAULT_MAX_FRAME_LEN).await
    }

    async fn write_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        response: Self::Response
    ) -> io::Result<()>
        where T: AsyncWrite + Unpin + Send
    {
        write_postcard_frame(io, &response, DEFAULT_MAX_FRAME_LEN).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration as StdDuration;

    use libp2p::{ Multiaddr, swarm::SwarmEvent };
    use rp_core::{
        crypto::{ Signer, SigningKey },
        models::transaction::{ Transaction, TransactionData },
    };
    use tempfile::tempdir;
    use tokio::{ sync::{ mpsc, oneshot }, time::timeout };

    use crate::network::config::DEFAULT_MAX_BLOCKS_PER_CHUNK;

    #[derive(NetworkBehaviour)]
    struct MockEmbeddedBehaviour {
        identify: identify::Behaviour,
        node_hello: request_response::Behaviour<NodeHelloCodec>,
        sync: request_response::Behaviour<SyncCodec>,
        announce: request_response::Behaviour<AnnounceCodec>,
    }

    enum MockPeerObservation {
        RuntimeNodeHelloVerified(NodePeerId),
        SyncRequest(SyncRequest),
        AnnounceRequest(AnnounceRequest),
    }

    #[derive(Clone, Copy)]
    enum MockNodeHelloResponseMode {
        Valid,
        InvalidSignature,
    }

    struct MockEmbeddedPeerHandle {
        node_peer_id: NodePeerId,
        listen_addr: Multiaddr,
        observation_rx: mpsc::Receiver<MockPeerObservation>,
    }

    struct MockEmbeddedPeer {
        swarm: Swarm<MockEmbeddedBehaviour>,
        node_identity: HostNodeIdentity,
        transport_peer_id: Vec<u8>,
        node_hello_response_mode: MockNodeHelloResponseMode,
        observation_tx: mpsc::Sender<MockPeerObservation>,
        ready_tx: Option<oneshot::Sender<Multiaddr>>,
        connected_runtime_peer: Option<PeerId>,
    }

    impl MockEmbeddedPeer {
        async fn spawn() -> MockEmbeddedPeerHandle {
            Self::spawn_with_node_hello_response_mode(MockNodeHelloResponseMode::Valid).await
        }

        async fn spawn_with_node_hello_response_mode(
            node_hello_response_mode: MockNodeHelloResponseMode
        ) -> MockEmbeddedPeerHandle {
            let node_identity = HostNodeIdentity::from_signing_key(SigningKey::random(&mut OsRng));
            let transport_keypair = identity::Keypair::generate_ed25519();
            let transport_peer_id = transport_keypair.public().to_peer_id().to_bytes();
            let node_peer_id = node_identity.peer_id;

            let (observation_tx, observation_rx) = mpsc::channel(16);
            let (ready_tx, ready_rx) = oneshot::channel();

            let mut swarm = SwarmBuilder::with_existing_identity(transport_keypair)
                .with_tokio()
                .with_tcp(tcp::Config::default(), noise::Config::new, yamux::Config::default)
                .expect("mock embedded peer tcp transport should build")
                .with_behaviour(|local_key| {
                    Ok(MockEmbeddedBehaviour {
                        identify: identify::Behaviour::new(
                            identify::Config
                                ::new(IDENTIFY_PROTOCOL_VERSION.to_string(), local_key.public())
                                .with_agent_version("erp-runtime-test/0.1.0".to_string())
                        ),
                        node_hello: request_response::Behaviour::new(
                            [(StreamProtocol::new(NODE_HELLO_PROTOCOL), ProtocolSupport::Full)],
                            request_response::Config
                                ::default()
                                .with_request_timeout(Duration::from_secs(10))
                        ),
                        sync: request_response::Behaviour::new(
                            [(StreamProtocol::new(SYNC_PROTOCOL), ProtocolSupport::Full)],
                            request_response::Config
                                ::default()
                                .with_request_timeout(Duration::from_secs(10))
                        ),
                        announce: request_response::Behaviour::new(
                            [(StreamProtocol::new(ANNOUNCE_PROTOCOL), ProtocolSupport::Full)],
                            request_response::Config
                                ::default()
                                .with_request_timeout(Duration::from_secs(10))
                        ),
                    })
                })
                .expect("mock embedded behaviour should build")
                .build();

            swarm
                .listen_on("/ip4/127.0.0.1/tcp/0".parse().expect("loopback multiaddr should parse"))
                .expect("mock embedded peer should listen");

            let mut peer = Self {
                swarm,
                node_identity,
                transport_peer_id,
                node_hello_response_mode,
                observation_tx,
                ready_tx: Some(ready_tx),
                connected_runtime_peer: None,
            };

            tokio::spawn(async move {
                peer.run().await;
            });

            let listen_addr = ready_rx.await.expect("mock peer should publish listen address");

            MockEmbeddedPeerHandle {
                node_peer_id,
                listen_addr,
                observation_rx,
            }
        }

        async fn run(&mut self) {
            loop {
                let event = self.swarm.select_next_some().await;
                self.handle_swarm_event(event).await;
            }
        }

        async fn handle_swarm_event(&mut self, event: SwarmEvent<MockEmbeddedBehaviourEvent>) {
            match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    if let Some(ready_tx) = self.ready_tx.take() {
                        let _ = ready_tx.send(address);
                    }
                }
                SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                    self.connected_runtime_peer = Some(peer_id);
                }
                SwarmEvent::Behaviour(MockEmbeddedBehaviourEvent::NodeHello(event)) => {
                    self.handle_node_hello_event(event).await;
                }
                SwarmEvent::Behaviour(MockEmbeddedBehaviourEvent::Sync(event)) => {
                    self.handle_sync_event(event).await;
                }
                SwarmEvent::Behaviour(MockEmbeddedBehaviourEvent::Announce(event)) => {
                    self.handle_announce_event(event).await;
                }
                _ => {}
            }
        }

        async fn handle_node_hello_event(
            &mut self,
            event: request_response::Event<NodeHello, NodeHelloResponse>
        ) {
            if let request_response::Event::Message { peer, message, .. } = event {
                match message {
                    request_response::Message::Request { request, channel, .. } => {
                        let verified = NodeHelloVerifier::verify(&request, &peer.to_bytes()).expect(
                            "runtime node hello should verify against transport peer"
                        );
                        self.observation_tx
                            .send(
                                MockPeerObservation::RuntimeNodeHelloVerified(verified.node_peer_id)
                            ).await
                            .expect("mock observation channel should stay open");
                        let response = self.build_node_hello_response();
                        self.swarm
                            .behaviour_mut()
                            .node_hello.send_response(channel, response)
                            .expect("mock peer should send node hello response");
                    }
                    request_response::Message::Response { .. } => {}
                }
            }
        }

        async fn handle_sync_event(
            &mut self,
            event: request_response::Event<SyncRequest, SyncResponse>
        ) {
            if let request_response::Event::Message { message, .. } = event {
                match message {
                    request_response::Message::Request { request, channel, .. } => {
                        self.observation_tx
                            .send(
                                MockPeerObservation::SyncRequest(SyncRequest {
                                    from_height: request.from_height,
                                    to_height: request.to_height,
                                })
                            ).await
                            .expect("mock observation channel should stay open");
                        self.swarm
                            .behaviour_mut()
                            .sync.send_response(channel, SyncResponse {
                                blocks: Vec::new(),
                                has_more: false,
                                next_height: None,
                            })
                            .expect("mock peer should send sync response");
                    }
                    request_response::Message::Response { .. } => {}
                }
            }
        }

        async fn handle_announce_event(
            &mut self,
            event: request_response::Event<AnnounceRequest, AnnounceResponse>
        ) {
            if let request_response::Event::Message { message, .. } = event {
                match message {
                    request_response::Message::Request { request, channel, .. } => {
                        self.observation_tx
                            .send(MockPeerObservation::AnnounceRequest(request)).await
                            .expect("mock observation channel should stay open");
                        self.swarm
                            .behaviour_mut()
                            .announce.send_response(channel, AnnounceResponse { accepted: true })
                            .expect("mock peer should send announce response");
                    }
                    request_response::Message::Response { .. } => {}
                }
            }
        }

        fn build_local_node_hello(&self) -> NodeHello {
            let capabilities = PeerCapabilities {
                supports_sync_v1: true,
                supports_announce_v1: true,
                supports_ping: false,
            };
            let transcript = NodeHelloTranscript {
                version: 1,
                node_peer_id: self.node_identity.peer_id,
                transport_peer_id: &self.transport_peer_id,
                max_frame_len: DEFAULT_MAX_FRAME_LEN,
                max_blocks_per_chunk: DEFAULT_MAX_BLOCKS_PER_CHUNK,
                capabilities: &capabilities,
            };
            let transcript_bytes = postcard
                ::to_allocvec(&transcript)
                .expect("mock node hello transcript should serialize");

            NodeHello {
                version: 1,
                node_public_key: self.node_identity.public_key.clone(),
                node_peer_id: self.node_identity.peer_id,
                transport_peer_id: self.transport_peer_id.clone(),
                max_frame_len: DEFAULT_MAX_FRAME_LEN,
                max_blocks_per_chunk: DEFAULT_MAX_BLOCKS_PER_CHUNK,
                capabilities,
                signature: self.node_identity.build_signature(&transcript_bytes),
            }
        }

        fn build_node_hello_response(&self) -> NodeHelloResponse {
            let mut remote = self.build_local_node_hello();
            if matches!(
                self.node_hello_response_mode,
                MockNodeHelloResponseMode::InvalidSignature
            ) {
                let signature_byte = remote.signature
                    .first_mut()
                    .expect("mock node hello signature should not be empty");
                *signature_byte ^= 0x01;
            }

            NodeHelloResponse {
                accepted: true,
                remote,
                reject_reason: None,
            }
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn rp_runtime_interops_with_embedded_protocol_peer() {
        let mock_peer = MockEmbeddedPeer::spawn().await;
        let temp_dir = tempdir().expect("temporary data dir should be created");

        let config = NetworkConfig {
            listen_addr: "/ip4/127.0.0.1/tcp/0".parse().expect("runtime listen addr should parse"),
            bootstrap_addrs: vec![mock_peer.listen_addr.clone()],
            ..NetworkConfig::default()
        };

        let (event_tx, mut event_rx) = mpsc::channel(32);
        let (network_tx, network_rx) = mpsc::channel(32);

        let mut network_manager = NetworkManager::new_with_config(
            network_rx,
            event_tx,
            temp_dir.path(),
            config
        ).expect("network manager should build for interop test");

        let network_task = tokio::spawn(async move { network_manager.run().await });

        let mut observation_rx = mock_peer.observation_rx;

        let runtime_node_peer_id = recv_observation(&mut observation_rx, |observation| {
            match observation {
                MockPeerObservation::RuntimeNodeHelloVerified(peer) => Some(peer),
                _ => None,
            }
        }).await;
        assert_ne!(runtime_node_peer_id, mock_peer.node_peer_id);

        let connected_peer = recv_runtime_event(&mut event_rx, |event| {
            match event {
                RuntimeEvent::PeerConnected { peer } => Some(peer),
                _ => None,
            }
        }).await;
        assert_eq!(connected_peer, mock_peer.node_peer_id);

        network_tx
            .send(NetworkCommand::RequestBlocks {
                peer: mock_peer.node_peer_id,
                from_height: 5,
                to_height: 9,
            }).await
            .expect("network command channel should stay open");

        let sync_request = recv_observation(&mut observation_rx, |observation| {
            match observation {
                MockPeerObservation::SyncRequest(request) => Some(request),
                _ => None,
            }
        }).await;
        assert_eq!(sync_request.from_height, 5);
        assert_eq!(sync_request.to_height, 9);

        let sync_response = recv_runtime_event(&mut event_rx, |event| {
            match event {
                RuntimeEvent::FrameReceived { peer, frame } if peer == mock_peer.node_peer_id => {
                    match NetworkMessage::from_bytes(&frame).expect("network frame should decode") {
                        NetworkMessage::SyncResponse(response) => Some(response),
                        _ => None,
                    }
                }
                _ => None,
            }
        }).await;
        assert!(sync_response.blocks.is_empty());
        assert!(!sync_response.has_more);
        assert_eq!(sync_response.next_height, None);

        let announce_request = sample_announce_request();
        let expected_announce_bytes = announce_request.to_bytes();
        network_tx
            .send(NetworkCommand::SendFrame {
                peer: mock_peer.node_peer_id,
                frame: NetworkMessage::AnnounceRequest(announce_request).to_bytes(),
            }).await
            .expect("network command channel should stay open");

        let observed_announce = recv_observation(&mut observation_rx, |observation| {
            match observation {
                MockPeerObservation::AnnounceRequest(request) => Some(request),
                _ => None,
            }
        }).await;
        assert_eq!(observed_announce.to_bytes(), expected_announce_bytes);

        let announce_response = recv_runtime_event(&mut event_rx, |event| {
            match event {
                RuntimeEvent::FrameReceived { peer, frame } if peer == mock_peer.node_peer_id => {
                    match NetworkMessage::from_bytes(&frame).expect("network frame should decode") {
                        NetworkMessage::AnnounceResponse(response) => Some(response),
                        _ => None,
                    }
                }
                _ => None,
            }
        }).await;
        assert!(announce_response.accepted);

        drop(network_tx);
        timeout(StdDuration::from_secs(5), network_task).await
            .expect("network task should stop after channel close")
            .expect("network task should join successfully")
            .expect("network manager should exit cleanly");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn rp_runtime_invalid_outbound_node_hello_response_is_peer_scoped() {
        let mock_peer = MockEmbeddedPeer::spawn_with_node_hello_response_mode(
            MockNodeHelloResponseMode::InvalidSignature
        ).await;
        let temp_dir = tempdir().expect("temporary data dir should be created");

        let config = NetworkConfig {
            listen_addr: "/ip4/127.0.0.1/tcp/0".parse().expect("runtime listen addr should parse"),
            bootstrap_addrs: vec![mock_peer.listen_addr.clone()],
            ..NetworkConfig::default()
        };

        let (event_tx, mut event_rx) = mpsc::channel(32);
        let (network_tx, network_rx) = mpsc::channel(32);

        let mut network_manager = NetworkManager::new_with_config(
            network_rx,
            event_tx,
            temp_dir.path(),
            config
        ).expect("network manager should build for invalid node hello test");

        let network_task = tokio::spawn(async move { network_manager.run().await });

        let mut observation_rx = mock_peer.observation_rx;

        let runtime_node_peer_id = recv_observation(&mut observation_rx, |observation| {
            match observation {
                MockPeerObservation::RuntimeNodeHelloVerified(peer) => Some(peer),
                _ => None,
            }
        }).await;
        assert_ne!(runtime_node_peer_id, mock_peer.node_peer_id);

        assert!(
            timeout(StdDuration::from_millis(750), event_rx.recv()).await.is_err(),
            "invalid node hello response should not emit runtime events"
        );
        assert!(
            !network_task.is_finished(),
            "invalid node hello response should not terminate the network manager"
        );

        drop(network_tx);
        timeout(StdDuration::from_secs(5), network_task).await
            .expect("network task should stop after channel close")
            .expect("network task should join successfully")
            .expect("network manager should exit cleanly");
    }

    async fn recv_runtime_event<T>(
        event_rx: &mut mpsc::Receiver<RuntimeEvent>,
        select: impl Fn(RuntimeEvent) -> Option<T>
    ) -> T {
        timeout(StdDuration::from_secs(5), async {
            loop {
                let event = event_rx.recv().await.expect("runtime event channel should stay open");
                if let Some(value) = select(event) {
                    return value;
                }
            }
        }).await.expect("expected runtime event within timeout")
    }

    async fn recv_observation<T>(
        observation_rx: &mut mpsc::Receiver<MockPeerObservation>,
        select: impl Fn(MockPeerObservation) -> Option<T>
    ) -> T {
        timeout(StdDuration::from_secs(5), async {
            loop {
                let observation = observation_rx
                    .recv().await
                    .expect("mock observation channel should stay open");
                if let Some(value) = select(observation) {
                    return value;
                }
            }
        }).await.expect("expected mock observation within timeout")
    }

    fn sample_announce_request() -> AnnounceRequest {
        let mut csprng = OsRng;
        let sender = SigningKey::random(&mut csprng);
        let receiver = SigningKey::random(&mut csprng);

        let mut transaction = Transaction {
            sender: sender.verifying_key().clone(),
            data: TransactionData::Transfer {
                receiver: receiver.verifying_key().clone(),
                amount: 42,
            },
            sequence: 7,
            fee: 1,
            signature: None,
        };
        let hash = transaction.hash();
        transaction.signature = Some(sender.sign(&hash));

        AnnounceRequest::transaction(transaction)
    }
}
