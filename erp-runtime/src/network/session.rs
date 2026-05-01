use std::{ collections::VecDeque, io::{ Error, ErrorKind }, sync::Arc };

use futures::{
    AsyncReadExt,
    AsyncWriteExt,
    FutureExt,
    SinkExt,
    StreamExt,
    channel::mpsc,
    executor::block_on,
    pin_mut,
    select,
};
use log::info;
use rp_core::traits::{ FromBytes, ToBytes };
use rp_node::network::message::{ AnnounceResponse, NetworkMessage, SyncResponse };
use serde::{ Serialize, de::DeserializeOwned };

use crate::{
    identity::manager::IdentityManager,
    network::{
        codec::{
            length_prefixed::encode_length_prefixed,
            postcard_codec::{ PostcardCodec, ValueCodec },
        },
        config::{ MultiaddrLite, NetworkConfig },
        peer_registry::SessionId,
        protocol::{
            announce::{
                ANNOUNCE_PROTOCOL,
                decode_announce_request,
                decode_announce_response,
                encode_announce_request,
                encode_announce_response,
            },
            identify::{ IDENTIFY_PROTOCOL, IdentifyInfo, decode_identify, encode_identify },
            node_hello::{
                NODE_HELLO_PROTOCOL,
                NodeHello,
                NodeHelloBuilder,
                NodeHelloResponse,
                NodeHelloVerifier,
                PeerCapabilities,
                VerifiedPeer,
            },
            sync::{ SYNC_PROTOCOL, decode_sync_request, decode_sync_response, encode_sync_request },
        },
        socket::esp_idf::EspTcpStream,
        transport::{
            multistream::{ dialer_select, listener_select },
            noise::{ self, NOISE_PROTOCOL },
            yamux::{ self, YamuxMuxer },
        },
        transport_identity::TransportIdentityManager,
    },
    runtime::{ errors::RuntimeError, manager::{ EventTx, RuntimeEvent } },
};

const IDENTIFY_PROTOCOL_VERSION: &str = "rust-proof/1";
const IDENTIFY_AGENT_VERSION: &str = "erp-runtime/0.1.0";
const SESSION_CONTROL_CHANNEL_CAPACITY: usize = 8;
const TAG: &str = "session";

pub enum SessionCommand {
    SendFrame(Vec<u8>),
    Disconnect,
}

#[derive(Debug, Clone)]
pub enum SessionEvent {
    Ready {
        session_id: SessionId,
        verified_peer: VerifiedPeer,
    },
    Closed {
        session_id: SessionId,
        node_peer_id: Option<[u8; 32]>,
    },
}

pub type SessionCommandTx = mpsc::Sender<SessionCommand>;
pub type SessionCommandRx = mpsc::Receiver<SessionCommand>;
pub type SessionEventTx = mpsc::UnboundedSender<SessionEvent>;
pub type SessionEventRx = mpsc::UnboundedReceiver<SessionEvent>;

pub struct SessionWorker {
    pub session_id: SessionId,
    pub stream: EspTcpStream,
    pub role: ConnectionRole,
    pub node_identity: Arc<IdentityManager>,
    pub transport_identity: Arc<TransportIdentityManager>,
    pub config: NetworkConfig,
    pub event_tx: EventTx,
    pub session_event_tx: SessionEventTx,
    pub command_rx: SessionCommandRx,
    pub expected_transport_peer: Option<Vec<u8>>,
}

pub enum ConnectionRole {
    Inbound,
    Outbound,
}

impl SessionWorker {
    pub fn run(self) -> Result<(), RuntimeError> {
        block_on(self.run_async())
    }

    async fn run_async(self) -> Result<(), RuntimeError> {
        let SessionWorker {
            session_id,
            stream,
            role,
            node_identity,
            transport_identity,
            config,
            event_tx,
            session_event_tx,
            mut command_rx,
            expected_transport_peer,
        } = self;

        let mut remote_node_peer_opt = None;
        let result = (async {
            info!(target: TAG, "session {} starting", session_id);
            let mut io = stream;
            info!(target: TAG, "session {} obtained futures IO", session_id);
            let noise_output = match role {
                ConnectionRole::Outbound => {
                    info!(target: TAG, "session {} selecting outbound noise protocol", session_id);
                    dialer_select(&mut io, NOISE_PROTOCOL).await?;
                    info!(target: TAG, "session {} upgrading outbound noise transport", session_id);
                    noise::upgrade_outbound(io, transport_identity.as_ref()).await?
                }
                ConnectionRole::Inbound => {
                    info!(target: TAG, "session {} selecting inbound noise protocol", session_id);
                    listener_select(&mut io, &[NOISE_PROTOCOL]).await?;
                    info!(target: TAG, "session {} upgrading inbound noise transport", session_id);
                    noise::upgrade_inbound(io, transport_identity.as_ref()).await?
                }
            };
            info!(target: TAG, "session {} completed noise handshake", session_id);

            if let Some(expected_transport_peer) = expected_transport_peer.as_ref() {
                if
                    expected_transport_peer.as_slice() !=
                    noise_output.remote_transport_peer_id.as_slice()
                {
                    return Err(
                        RuntimeError::config(
                            "bootstrap peer transport identity did not match expectation"
                        )
                    );
                }
            }

            let mut yamux_session = match role {
                ConnectionRole::Outbound => {
                    info!(target: TAG, "session {} upgrading outbound yamux", session_id);
                    yamux::upgrade_outbound(noise_output.stream).await?
                }
                ConnectionRole::Inbound => {
                    info!(target: TAG, "session {} upgrading inbound yamux", session_id);
                    yamux::upgrade_inbound(noise_output.stream).await?
                }
            };
            info!(target: TAG, "session {} completed yamux setup", session_id);

            let verified_peer = match role {
                ConnectionRole::Outbound => {
                    info!(target: TAG, "session {} running outbound identify/node-hello handshake", session_id);
                    complete_outbound_handshake(
                        &mut yamux_session.muxer,
                        node_identity.as_ref(),
                        transport_identity.as_ref(),
                        &config,
                        &noise_output.remote_transport_peer_id
                    ).await?
                }
                ConnectionRole::Inbound => {
                    info!(target: TAG, "session {} running inbound identify/node-hello handshake", session_id);
                    complete_inbound_handshake(
                        &mut yamux_session.muxer,
                        node_identity.as_ref(),
                        transport_identity.as_ref(),
                        &config,
                        &noise_output.remote_transport_peer_id
                    ).await?
                }
            };
            info!(target: TAG, "session {} verified remote node peer {:?}", session_id, verified_peer.node_peer_id);

            send_session_event(&session_event_tx, SessionEvent::Ready {
                session_id,
                verified_peer: verified_peer.clone(),
            })?;

            let remote_node_peer = verified_peer.node_peer_id;
            remote_node_peer_opt = Some(remote_node_peer);
            let mut deferred_commands = VecDeque::new();
            loop {
                enum LoopEvent {
                    Command(Option<SessionCommand>),
                    Inbound(Result<Option<::yamux::Stream>, RuntimeError>),
                }

                let next_event = if let Some(command) = deferred_commands.pop_front() {
                    LoopEvent::Command(Some(command))
                } else {
                    let next_command = command_rx.next().fuse();
                    let next_inbound = yamux_session.muxer.accept_substream().fuse();
                    pin_mut!(next_command, next_inbound);

                    select! {
                        command = next_command => LoopEvent::Command(command),
                        inbound = next_inbound => LoopEvent::Inbound(inbound),
                    }
                };

                match next_event {
                    LoopEvent::Command(Some(SessionCommand::SendFrame(frame))) => {
                        handle_outbound_frame(
                            &mut yamux_session.muxer,
                            &config,
                            &mut event_tx.clone(),
                            remote_node_peer,
                            frame
                        ).await?;
                    }
                    LoopEvent::Command(Some(SessionCommand::Disconnect)) => {
                        break;
                    }
                    LoopEvent::Command(None) => {
                        break;
                    }
                    LoopEvent::Inbound(Ok(Some(substream))) => {
                        handle_inbound_substream(
                            substream,
                            node_identity.as_ref(),
                            transport_identity.as_ref(),
                            &config,
                            &mut event_tx.clone(),
                            &verified_peer,
                            &mut command_rx,
                            &mut deferred_commands
                        ).await?;
                    }
                    LoopEvent::Inbound(Ok(None)) => {
                        break;
                    }
                    LoopEvent::Inbound(Err(error)) => {
                        return Err(error);
                    }
                }
            }

            let _ = yamux_session.muxer.close().await;
            Ok(())
        }).await;

        let _ = send_session_event(&session_event_tx, SessionEvent::Closed {
            session_id,
            node_peer_id: remote_node_peer_opt.take(),
        });

        result
    }
}

pub fn session_command_channel() -> (SessionCommandTx, SessionCommandRx) {
    mpsc::channel(SESSION_CONTROL_CHANNEL_CAPACITY)
}

async fn complete_outbound_handshake<S>(
    muxer: &mut YamuxMuxer<S>,
    node_identity: &IdentityManager,
    transport_identity: &TransportIdentityManager,
    config: &NetworkConfig,
    authenticated_transport_peer: &[u8]
) -> Result<VerifiedPeer, RuntimeError>
    where S: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin
{
    let identify = request_identify(muxer, config, authenticated_transport_peer).await?;
    if identify.transport_peer_id.as_slice() != authenticated_transport_peer {
        return Err(
            RuntimeError::config(
                "identify transport peer did not match the authenticated transport session"
            )
        );
    }

    request_node_hello(
        muxer,
        node_identity,
        transport_identity,
        config,
        authenticated_transport_peer
    ).await
}

async fn complete_inbound_handshake<S>(
    muxer: &mut YamuxMuxer<S>,
    node_identity: &IdentityManager,
    transport_identity: &TransportIdentityManager,
    config: &NetworkConfig,
    authenticated_transport_peer: &[u8]
) -> Result<VerifiedPeer, RuntimeError>
    where S: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin
{
    let mut identify_served = false;
    let mut verified_peer = None;

    while !identify_served || verified_peer.is_none() {
        let Some(mut substream) = muxer.accept_substream().await? else {
            return Err(
                RuntimeError::config("remote closed the connection before session setup completed")
            );
        };

        let protocol = listener_select(
            &mut substream,
            &[IDENTIFY_PROTOCOL, NODE_HELLO_PROTOCOL]
        ).await?;

        match protocol.as_str() {
            IDENTIFY_PROTOCOL => {
                send_identify(&mut substream, config, transport_identity).await?;
                identify_served = true;
            }
            NODE_HELLO_PROTOCOL => {
                verified_peer = Some(
                    answer_node_hello(
                        &mut substream,
                        node_identity,
                        transport_identity,
                        config,
                        authenticated_transport_peer
                    ).await?
                );
            }
            _ => {
                return Err(RuntimeError::config("unexpected session setup protocol"));
            }
        }
    }

    verified_peer.ok_or_else(|| RuntimeError::config("node hello handshake did not complete"))
}

async fn request_identify<S>(
    muxer: &mut YamuxMuxer<S>,
    config: &NetworkConfig,
    authenticated_transport_peer: &[u8]
) -> Result<IdentifyInfo, RuntimeError>
    where S: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin
{
    let mut substream = muxer.open_substream().await?;
    dialer_select(&mut substream, IDENTIFY_PROTOCOL).await?;
    let frame = read_length_prefixed_frame(&mut substream, config.max_frame_len).await?;
    let identify = decode_identify(&frame)?;
    if identify.transport_peer_id.as_slice() != authenticated_transport_peer {
        return Err(
            RuntimeError::config(
                "identify transport peer id did not match the authenticated transport session"
            )
        );
    }
    Ok(identify)
}

async fn send_identify<S>(
    stream: &mut S,
    config: &NetworkConfig,
    transport_identity: &TransportIdentityManager
) -> Result<(), RuntimeError>
    where S: futures::io::AsyncWrite + Unpin
{
    let identify = build_local_identify(config, transport_identity);
    let frame = encode_identify(&identify)?;
    stream.write_all(&frame).await.map_err(RuntimeError::NetworkError)?;
    stream.flush().await.map_err(RuntimeError::NetworkError)
}

async fn request_node_hello<S>(
    muxer: &mut YamuxMuxer<S>,
    node_identity: &IdentityManager,
    transport_identity: &TransportIdentityManager,
    config: &NetworkConfig,
    authenticated_transport_peer: &[u8]
) -> Result<VerifiedPeer, RuntimeError>
    where S: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin
{
    let mut substream = muxer.open_substream().await?;
    dialer_select(&mut substream, NODE_HELLO_PROTOCOL).await?;

    let local_hello = build_node_hello(node_identity, transport_identity, config)?;
    write_postcard_frame(&mut substream, &local_hello, config.max_frame_len).await?;
    let response: NodeHelloResponse = read_postcard_frame(
        &mut substream,
        config.max_frame_len
    ).await?;
    if !response.accepted {
        return Err(RuntimeError::config("remote rejected the node hello handshake"));
    }

    NodeHelloVerifier::verify(&response.remote, authenticated_transport_peer)
}

async fn answer_node_hello<S>(
    stream: &mut S,
    node_identity: &IdentityManager,
    transport_identity: &TransportIdentityManager,
    config: &NetworkConfig,
    authenticated_transport_peer: &[u8]
) -> Result<VerifiedPeer, RuntimeError>
    where S: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin
{
    let remote_hello: NodeHello = read_postcard_frame(stream, config.max_frame_len).await?;
    let verified_peer = NodeHelloVerifier::verify(&remote_hello, authenticated_transport_peer)?;
    let local_hello = build_node_hello(node_identity, transport_identity, config)?;
    let response = NodeHelloResponse {
        accepted: true,
        remote: local_hello,
        reject_reason: None,
    };
    write_postcard_frame(stream, &response, config.max_frame_len).await?;
    Ok(verified_peer)
}

async fn handle_outbound_frame<S>(
    muxer: &mut YamuxMuxer<S>,
    config: &NetworkConfig,
    event_tx: &mut EventTx,
    remote_node_peer: [u8; 32],
    frame: Vec<u8>
) -> Result<(), RuntimeError>
    where S: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin
{
    let message = NetworkMessage::from_bytes(&frame).map_err(|_|
        RuntimeError::config("invalid outbound network frame")
    )?;

    match message {
        NetworkMessage::AnnounceRequest(request) => {
            let mut substream = muxer.open_substream().await?;
            dialer_select(&mut substream, ANNOUNCE_PROTOCOL).await?;
            let payload = encode_announce_request(&request, config.max_frame_len)?;
            substream.write_all(&payload).await.map_err(RuntimeError::NetworkError)?;
            substream.flush().await.map_err(RuntimeError::NetworkError)?;

            let response_frame = read_length_prefixed_frame(
                &mut substream,
                config.max_frame_len
            ).await?;
            let response = decode_announce_response(&response_frame, config.max_frame_len)?;
            event_tx
                .send(RuntimeEvent::FrameReceived {
                    peer: remote_node_peer,
                    frame: NetworkMessage::AnnounceResponse(response).to_bytes(),
                }).await
                .map_err(RuntimeError::event_send)?;
            Ok(())
        }
        NetworkMessage::SyncRequest(request) => {
            let mut substream = muxer.open_substream().await?;
            dialer_select(&mut substream, SYNC_PROTOCOL).await?;
            let payload = encode_sync_request(&request, config.max_frame_len)?;
            substream.write_all(&payload).await.map_err(RuntimeError::NetworkError)?;
            substream.flush().await.map_err(RuntimeError::NetworkError)?;

            let response_frame = read_length_prefixed_frame(
                &mut substream,
                config.max_frame_len
            ).await?;
            let response = decode_sync_response(&response_frame, config.max_frame_len)?;
            event_tx
                .send(RuntimeEvent::FrameReceived {
                    peer: remote_node_peer,
                    frame: NetworkMessage::SyncResponse(response).to_bytes(),
                }).await
                .map_err(RuntimeError::event_send)?;
            Ok(())
        }
        NetworkMessage::AnnounceResponse(_) | NetworkMessage::SyncResponse(_) => {
            Err(
                RuntimeError::config(
                    "outbound response routing is not implemented for session commands"
                )
            )
        }
    }
}

async fn handle_inbound_substream<S>(
    mut substream: S,
    node_identity: &IdentityManager,
    transport_identity: &TransportIdentityManager,
    config: &NetworkConfig,
    event_tx: &mut EventTx,
    verified_peer: &VerifiedPeer,
    command_rx: &mut SessionCommandRx,
    deferred_commands: &mut VecDeque<SessionCommand>
) -> Result<(), RuntimeError>
    where S: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin
{
    let protocol = listener_select(
        &mut substream,
        &[IDENTIFY_PROTOCOL, NODE_HELLO_PROTOCOL, ANNOUNCE_PROTOCOL, SYNC_PROTOCOL]
    ).await?;

    match protocol.as_str() {
        IDENTIFY_PROTOCOL => send_identify(&mut substream, config, transport_identity).await,
        NODE_HELLO_PROTOCOL => {
            let remote = answer_node_hello(
                &mut substream,
                node_identity,
                transport_identity,
                config,
                verified_peer.transport_peer_id.as_slice()
            ).await?;
            if remote.node_peer_id != verified_peer.node_peer_id {
                return Err(
                    RuntimeError::config("node hello peer changed during an existing session")
                );
            }
            Ok(())
        }
        ANNOUNCE_PROTOCOL => {
            let frame = read_length_prefixed_frame(&mut substream, config.max_frame_len).await?;
            let request = decode_announce_request(&frame, config.max_frame_len)?;
            event_tx
                .send(RuntimeEvent::FrameReceived {
                    peer: verified_peer.node_peer_id,
                    frame: NetworkMessage::AnnounceRequest(request).to_bytes(),
                }).await
                .map_err(RuntimeError::event_send)?;

            let response = AnnounceResponse { accepted: true };
            let response_frame = encode_announce_response(&response, config.max_frame_len)?;
            substream.write_all(&response_frame).await.map_err(RuntimeError::NetworkError)?;
            substream.flush().await.map_err(RuntimeError::NetworkError)
        }
        SYNC_PROTOCOL => {
            let frame = read_length_prefixed_frame(&mut substream, config.max_frame_len).await?;
            let request = decode_sync_request(&frame, config.max_frame_len)?;
            event_tx
                .send(RuntimeEvent::FrameReceived {
                    peer: verified_peer.node_peer_id,
                    frame: NetworkMessage::SyncRequest(request).to_bytes(),
                }).await
                .map_err(RuntimeError::event_send)?;

            let response = wait_for_sync_response(command_rx, deferred_commands).await?;
            let response_frame = crate::network::protocol::sync::encode_sync_response(
                &response,
                config.max_frame_len
            )?;
            substream.write_all(&response_frame).await.map_err(RuntimeError::NetworkError)?;
            substream.flush().await.map_err(RuntimeError::NetworkError)
        }
        _ => Err(RuntimeError::config("unsupported inbound protocol")),
    }
}

async fn wait_for_sync_response(
    command_rx: &mut SessionCommandRx,
    deferred_commands: &mut VecDeque<SessionCommand>
) -> Result<SyncResponse, RuntimeError> {
    loop {
        let command = if let Some(command) = deferred_commands.pop_front() {
            Some(command)
        } else {
            command_rx.next().await
        };

        match command {
            Some(SessionCommand::SendFrame(frame)) => {
                let message = NetworkMessage::from_bytes(&frame).map_err(|_| {
                    RuntimeError::config("invalid runtime frame while waiting for sync response")
                })?;

                match message {
                    NetworkMessage::SyncResponse(response) => {
                        return Ok(response);
                    }
                    other => {
                        deferred_commands.push_back(SessionCommand::SendFrame(other.to_bytes()));
                    }
                }
            }
            Some(SessionCommand::Disconnect) => {
                return Err(
                    RuntimeError::config("session disconnected while waiting for sync response")
                );
            }
            None => {
                return Err(
                    RuntimeError::config(
                        "session command channel closed while waiting for sync response"
                    )
                );
            }
        }
    }
}

fn build_local_identify(
    config: &NetworkConfig,
    transport_identity: &TransportIdentityManager
) -> IdentifyInfo {
    IdentifyInfo {
        protocol_version: IDENTIFY_PROTOCOL_VERSION.into(),
        agent_version: IDENTIFY_AGENT_VERSION.into(),
        listen_addrs: vec![MultiaddrLite::Ip4Tcp {
            addr: [0, 0, 0, 0],
            port: config.listen_port,
        }],
        supported_protocols: vec![
            IDENTIFY_PROTOCOL.into(),
            NODE_HELLO_PROTOCOL.into(),
            SYNC_PROTOCOL.into(),
            ANNOUNCE_PROTOCOL.into()
        ],
        observed_addr: None,
        transport_public_key: transport_identity.public_key_protobuf_bytes().to_vec(),
        transport_peer_id: transport_identity.peer_id_bytes().to_vec(),
    }
}

fn build_node_hello(
    node_identity: &IdentityManager,
    transport_identity: &TransportIdentityManager,
    config: &NetworkConfig
) -> Result<NodeHello, RuntimeError> {
    (NodeHelloBuilder {
        node_identity,
        transport_identity,
        max_frame_len: config.max_frame_len,
        max_blocks_per_chunk: config.max_blocks_per_chunk,
        capabilities: PeerCapabilities {
            supports_sync_v1: true,
            supports_announce_v1: true,
            supports_ping: false,
        },
    }).build()
}

async fn write_postcard_frame<S, T>(
    stream: &mut S,
    value: &T,
    max_len: u32
)
    -> Result<(), RuntimeError>
    where S: futures::io::AsyncWrite + Unpin, T: Serialize + DeserializeOwned
{
    let payload = PostcardCodec::<T>::encode(value)?;
    let frame = encode_length_prefixed(&payload, max_len)?;
    stream.write_all(&frame).await.map_err(RuntimeError::NetworkError)?;
    stream.flush().await.map_err(RuntimeError::NetworkError)
}

async fn read_postcard_frame<S, T>(stream: &mut S, max_len: u32) -> Result<T, RuntimeError>
    where S: futures::io::AsyncRead + Unpin, T: Serialize + DeserializeOwned
{
    let frame = read_length_prefixed_frame(stream, max_len).await?;
    let payload = decode_length_prefixed_payload(&frame, max_len)?;
    PostcardCodec::<T>::decode(payload)
}

async fn read_length_prefixed_frame<S>(
    stream: &mut S,
    max_len: u32
) -> Result<Vec<u8>, RuntimeError>
    where S: futures::io::AsyncRead + Unpin
{
    let mut prefix = [0u8; 5];
    let mut prefix_len = 0usize;
    loop {
        if prefix_len >= prefix.len() {
            return Err(
                RuntimeError::NetworkError(
                    Error::new(
                        ErrorKind::InvalidData,
                        "frame length prefix exceeds u32 varint width"
                    )
                )
            );
        }

        stream
            .read_exact(&mut prefix[prefix_len..=prefix_len]).await
            .map_err(RuntimeError::NetworkError)?;
        prefix_len += 1;

        if (prefix[prefix_len - 1] & 0x80) == 0 {
            let payload = decode_length_prefixed_payload_len(&prefix[..prefix_len], max_len)?;
            let mut frame = Vec::with_capacity(prefix_len + payload);
            frame.extend_from_slice(&prefix[..prefix_len]);
            frame.resize(prefix_len + payload, 0);
            stream.read_exact(&mut frame[prefix_len..]).await.map_err(RuntimeError::NetworkError)?;
            return Ok(frame);
        }
    }
}

fn decode_length_prefixed_payload<'a>(
    frame: &'a [u8],
    max_len: u32
) -> Result<&'a [u8], RuntimeError> {
    crate::network::codec::length_prefixed
        ::decode_length_prefix(frame, max_len)
        .map(|(_, payload)| payload)
}

fn decode_length_prefixed_payload_len(prefix: &[u8], max_len: u32) -> Result<usize, RuntimeError> {
    unsigned_varint::decode
        ::u32(prefix)
        .map(|(len, _)| len as usize)
        .map_err(|error| {
            RuntimeError::NetworkError(
                Error::new(ErrorKind::InvalidData, format!("invalid frame length prefix: {error}"))
            )
        })
        .and_then(|len| {
            if len > (max_len as usize) {
                Err(
                    RuntimeError::NetworkError(
                        Error::new(
                            ErrorKind::InvalidData,
                            "frame length exceeds configured maximum"
                        )
                    )
                )
            } else {
                Ok(len)
            }
        })
}

fn send_session_event(
    session_event_tx: &SessionEventTx,
    event: SessionEvent
) -> Result<(), RuntimeError> {
    session_event_tx
        .unbounded_send(event)
        .map_err(|_| RuntimeError::config("failed to send session event to network manager"))
}
