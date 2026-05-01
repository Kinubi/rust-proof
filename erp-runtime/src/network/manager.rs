use std::{ collections::BTreeMap, io::Error, net::SocketAddr, pin::Pin, sync::Arc, thread };

use embassy_time::{ Duration, Timer };
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{ AsyncWifi, EspWifi };
use futures::{ FutureExt, SinkExt, StreamExt, pin_mut, select };
use libp2p_identity::PeerId as Libp2pPeerId;
use log::{ error, info, warn };
use multiaddr::{ Multiaddr, Protocol };
use rp_core::traits::ToBytes;
use rp_node::{ contract::PeerId, network::message::{ NetworkMessage, SyncRequest } };

use crate::{
    identity::manager::IdentityManager,
    network::{
        bootstrap::{ bootstrap_targets, resolve_bootstrap_addr },
        config::NetworkConfig,
        peer_registry::{ PeerRegistry, SessionState },
        session::{
            ConnectionRole,
            SessionCommand,
            SessionCommandTx,
            SessionEvent,
            SessionEventTx,
            SessionWorker,
            session_command_channel,
        },
        socket::{ esp_idf::EspSocketFactory, traits::SocketFactory },
        transport_identity::TransportIdentityManager,
    },
    runtime::{
        errors::RuntimeError,
        manager::{ EventTx, NetworkCommand, NetworkRx, RuntimeEvent },
    },
};

const TAG: &str = "manager";
const DEFAULT_LISTEN_PORT: u16 = 4001;
const DEFAULT_MAX_PEERS: usize = 16;
const DEFAULT_MAX_OUTBOUND_DIALS: usize = 4;
const DEFAULT_MAX_FRAME_LEN: u32 = 64 * 1024;
const DEFAULT_MAX_BLOCKS_PER_CHUNK: u16 = 32;
const DEFAULT_IDLE_TIMEOUT_MS: u64 = 60_000;
const BOOTSTRAP_RETRY_MS: u64 = 10_000;
const SESSION_THREAD_STACK_SIZE: usize = 96 * 1024;
const BOOTSTRAP_PEERS_ENV: Option<&str> = option_env!("BOOTSTRAP_PEERS");

pub struct NetworkManager {
    network_rx: NetworkRx,
    event_tx: EventTx,
    node_identity: Arc<IdentityManager>,
    transport_identity: Arc<TransportIdentityManager>,
    config: NetworkConfig,
    peers: PeerRegistry,
    session_commands: BTreeMap<usize, SessionCommandTx>,
    wifi: AsyncWifi<EspWifi<'static>>,
}

impl NetworkManager {
    pub fn new(
        network_rx: NetworkRx,
        event_tx: EventTx,
        identity: IdentityManager,
        wifi: AsyncWifi<EspWifi<'static>>,
        nvs_partition: EspDefaultNvsPartition
    ) -> Result<Self, RuntimeError> {
        let config = default_network_config()?;
        let transport_identity = TransportIdentityManager::load_or_create(nvs_partition)?;

        Ok(Self {
            network_rx,
            event_tx,
            node_identity: Arc::new(identity),
            transport_identity: Arc::new(transport_identity),
            peers: PeerRegistry::new(config.max_peers),
            session_commands: BTreeMap::new(),
            config,
            wifi,
        })
    }

    async fn ensure_wifi_connected(&mut self) -> Result<(), RuntimeError> {
        if !self.wifi.is_started().map_err(RuntimeError::esp)? {
            self.wifi.start().await.map_err(RuntimeError::esp)?;
            info!(target: TAG, "wifi started");
        }

        (
            unsafe {
                esp_idf_hal::sys::esp!(
                    esp_idf_hal::sys::esp_wifi_set_ps(esp_idf_hal::sys::wifi_ps_type_t_WIFI_PS_NONE)
                )
            }
        ).map_err(RuntimeError::esp)?;

        if !self.wifi.is_connected().map_err(RuntimeError::esp)? {
            self.wifi.connect().await.map_err(RuntimeError::esp)?;
            info!(target: TAG, "wifi connected");
        }

        self.wifi.wait_netif_up().await.map_err(RuntimeError::esp)?;
        info!(target: TAG, "wifi netif up");

        // Give the network stack a moment to fully stabilize (ARP, etc.)
        Timer::after(Duration::from_secs(2)).await;
        info!(target: TAG, "post-netif delay complete");

        Ok(())
    }

    pub async fn run(&mut self) -> Result<(), RuntimeError> {
        info!(target: TAG, "running network manager");

        self.ensure_wifi_connected().await?;

        let ip_info = self.wifi.wifi().sta_netif().get_ip_info().map_err(RuntimeError::esp)?;
        let local_addr = SocketAddr::from((ip_info.ip, self.config.listen_port));
        info!(target: TAG, "using station ip {local_addr}");
        info!(target: TAG, "gateway: {}, netmask: {}", ip_info.subnet.gateway, ip_info.subnet.mask);

        // Log the MAC address of the sta_netif
        match self.wifi.wifi().sta_netif().get_mac() {
            Ok(mac) =>
                info!(target: TAG, "sta_netif MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}", 
                mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]),
            Err(e) => warn!(target: TAG, "failed to get sta_netif MAC: {:?}", e),
        }

        // Test UDP unicast to gateway to check if basic outbound works
        info!(target: TAG, "testing UDP unicast to gateway...");
        match std::net::UdpSocket::bind("0.0.0.0:0") {
            Ok(udp_sock) => {
                let gateway_addr = format!("{}:53", ip_info.subnet.gateway);
                match udp_sock.send_to(b"test", &gateway_addr) {
                    Ok(sent) => info!(target: TAG, "UDP send to gateway succeeded ({sent} bytes)"),
                    Err(e) => warn!(target: TAG, "UDP send to gateway failed: {:?}", e),
                }
            }
            Err(e) => warn!(target: TAG, "UDP socket bind failed: {:?}", e),
        }

        // Test TCP connect to gateway port 80 (router usually has web interface)
        info!(target: TAG, "testing TCP connect to gateway:80...");
        let gateway_tcp = format!("{}:80", ip_info.subnet.gateway);
        match
            std::net::TcpStream::connect_timeout(
                &gateway_tcp.parse().unwrap(),
                std::time::Duration::from_secs(3)
            )
        {
            Ok(_) => info!(target: TAG, "TCP connect to gateway:80 succeeded!"),
            Err(e) =>
                warn!(target: TAG, "TCP connect to gateway:80 failed: {:?} (os_error={:?})", e.kind(), e.raw_os_error()),
        }

        let sockets = EspSocketFactory::new(local_addr.into());
        let mut listener = sockets.bind(self.config.listen_port).await?;
        let (session_event_tx, mut session_event_rx) = futures::channel::mpsc::unbounded();
        let mut bootstrap_cursor = 0usize;
        let mut bootstrap_timer: Pin<Box<_>> = Box::pin(
            Timer::after(Duration::from_millis(0)).fuse()
        );

        loop {
            let next_accept = sockets.accept(&mut listener).fuse();
            let next_command = self.network_rx.next().fuse();
            let next_session_event = session_event_rx.next().fuse();
            pin_mut!(next_accept, next_command, next_session_event);

            select! {
                accepted = next_accept => {
                    match accepted {
                        Ok((stream, remote_addr)) => {
                            info!(target: TAG, "accepted inbound tcp connection from {:?}", remote_addr);
                            self.spawn_session(
                                stream,
                                ConnectionRole::Inbound,
                                &session_event_tx,
                                None,
                                None,
                            )?;
                        }
                        Err(error) => {
                            warn!(target: TAG, "accept loop error: {:?}", error);
                        }
                    }
                }
                command = next_command => {
                    let Some(command) = command else {
                        info!(target: TAG, "network command channel closed");
                        break;
                    };
                    self.handle_network_command(command).await?;
                }
                session_event = next_session_event => {
                    if let Some(session_event) = session_event {
                        self.handle_session_event(session_event).await?;
                    }
                }
                _ = bootstrap_timer.as_mut() => {
                    self.bootstrap_once(&sockets, &session_event_tx, &mut bootstrap_cursor).await?;
                    bootstrap_timer = Box::pin(Timer::after(Duration::from_millis(BOOTSTRAP_RETRY_MS)).fuse());
                }
            }
        }

        Ok(())
    }

    async fn handle_network_command(
        &mut self,
        command: NetworkCommand
    ) -> Result<(), RuntimeError> {
        match command {
            NetworkCommand::SendFrame { peer, frame } => self.send_frame_to_peer(peer, frame).await,
            NetworkCommand::BroadcastFrame { frame } => {
                for session_id in self.peers.ready_sessions() {
                    if let Some(command_tx) = self.session_commands.get_mut(&session_id) {
                        command_tx
                            .send(SessionCommand::SendFrame(frame.clone())).await
                            .map_err(RuntimeError::network_send)?;
                    }
                }
                Ok(())
            }
            NetworkCommand::DisconnectPeer { peer } => self.disconnect_peer(peer).await,
            NetworkCommand::RequestBlocks { peer, from_height, to_height } => {
                let frame = NetworkMessage::SyncRequest(SyncRequest {
                    from_height,
                    to_height,
                }).to_bytes();
                self.send_frame_to_peer(peer, frame).await
            }
        }
    }

    async fn send_frame_to_peer(
        &mut self,
        peer: PeerId,
        frame: Vec<u8>
    ) -> Result<(), RuntimeError> {
        let Some(session_id) = self.peers.session_for_node(&peer) else {
            warn!(target: TAG, "dropping outbound frame for unknown peer {:?}", peer);
            return Ok(());
        };

        let Some(command_tx) = self.session_commands.get_mut(&session_id) else {
            warn!(target: TAG, "dropping outbound frame for missing session {}", session_id);
            return Ok(());
        };

        command_tx.send(SessionCommand::SendFrame(frame)).await.map_err(RuntimeError::network_send)
    }

    async fn disconnect_peer(&mut self, peer: PeerId) -> Result<(), RuntimeError> {
        let Some(session_id) = self.peers.session_for_node(&peer) else {
            return Ok(());
        };
        let Some(command_tx) = self.session_commands.get_mut(&session_id) else {
            return Ok(());
        };

        command_tx.send(SessionCommand::Disconnect).await.map_err(RuntimeError::network_send)
    }

    async fn handle_session_event(&mut self, event: SessionEvent) -> Result<(), RuntimeError> {
        match event {
            SessionEvent::Ready { session_id, verified_peer } => {
                let session = self.peers
                    .get_mut(session_id)
                    .ok_or_else(|| {
                        RuntimeError::config("session ready event referenced an unknown session")
                    })?;
                session.transport_peer_id = verified_peer.transport_peer_id.clone();
                session.max_frame_len = verified_peer.max_frame_len;
                session.max_blocks_per_chunk = verified_peer.max_blocks_per_chunk;
                session.state = SessionState::NodeReady;
                self.peers.register_node_peer(session_id, verified_peer.node_peer_id)?;

                self.event_tx
                    .send(RuntimeEvent::PeerConnected {
                        peer: verified_peer.node_peer_id,
                    }).await
                    .map_err(RuntimeError::event_send)?;
            }
            SessionEvent::Closed { session_id, node_peer_id } => {
                self.session_commands.remove(&session_id);
                self.peers.remove(session_id);

                if let Some(node_peer_id) = node_peer_id {
                    self.event_tx
                        .send(RuntimeEvent::PeerDisconnected { peer: node_peer_id }).await
                        .map_err(RuntimeError::event_send)?;
                }
            }
        }

        Ok(())
    }

    async fn bootstrap_once(
        &mut self,
        sockets: &EspSocketFactory,
        session_event_tx: &SessionEventTx,
        bootstrap_cursor: &mut usize
    ) -> Result<(), RuntimeError> {
        if self.peers.outbound_session_count() >= self.config.max_outbound_dials {
            return Ok(());
        }

        let targets = bootstrap_targets(&self.config);
        if targets.is_empty() {
            return Ok(());
        }

        let target = &targets[*bootstrap_cursor % targets.len()];
        *bootstrap_cursor = bootstrap_cursor.saturating_add(1);
        let address = match resolve_bootstrap_addr(&target.address) {
            Ok(address) => address,
            Err(error) => {
                warn!(target: TAG, "failed to resolve bootstrap target: {:?}", error);
                return Ok(());
            }
        };

        if self.peers.has_outbound_session_for(&address) {
            return Ok(());
        }

        match sockets.connect(address.into()).await {
            Ok(stream) => {
                info!(target: TAG, "connected to bootstrap peer {address}");
                self.spawn_session(
                    stream,
                    ConnectionRole::Outbound,
                    session_event_tx,
                    Some(address),
                    target.expected_transport_peer.clone()
                )?;
            }
            Err(error) => {
                warn!(target: TAG, "bootstrap dial to {address} failed: {:?}", error);
            }
        }

        Ok(())
    }

    fn spawn_session(
        &mut self,
        stream: crate::network::socket::esp_idf::EspTcpStream,
        role: ConnectionRole,
        session_event_tx: &SessionEventTx,
        outbound_addr: Option<SocketAddr>,
        expected_transport_peer: Option<Vec<u8>>
    ) -> Result<(), RuntimeError> {
        let session_id = self.peers.alloc(
            Vec::new(),
            matches!(role, ConnectionRole::Outbound),
            outbound_addr
        )?;
        let (command_tx, command_rx) = session_command_channel();
        let worker = SessionWorker {
            session_id,
            stream,
            role,
            node_identity: Arc::clone(&self.node_identity),
            transport_identity: Arc::clone(&self.transport_identity),
            config: self.config.clone(),
            event_tx: self.event_tx.clone(),
            session_event_tx: session_event_tx.clone(),
            command_rx,
            expected_transport_peer,
        };

        self.session_commands.insert(session_id, command_tx);

        thread::Builder
            ::new()
            .name(format!("net-session-{session_id}"))
            .stack_size(SESSION_THREAD_STACK_SIZE)
            .spawn(move || {
                if let Err(error) = worker.run() {
                    error!(target: TAG, "session {} failed: {:?}", session_id, error);
                }
            })
            .map(|_| ())
            .map_err(|error| RuntimeError::NetworkError(Error::other(error.to_string())))
    }
}

fn default_network_config() -> Result<NetworkConfig, RuntimeError> {
    Ok(NetworkConfig {
        listen_port: DEFAULT_LISTEN_PORT,
        max_peers: DEFAULT_MAX_PEERS,
        max_outbound_dials: DEFAULT_MAX_OUTBOUND_DIALS,
        max_frame_len: DEFAULT_MAX_FRAME_LEN,
        max_blocks_per_chunk: DEFAULT_MAX_BLOCKS_PER_CHUNK,
        idle_timeout_ms: DEFAULT_IDLE_TIMEOUT_MS,
        bootstrap_peers: parse_bootstrap_peers(BOOTSTRAP_PEERS_ENV)?,
    })
}

fn parse_bootstrap_peers(
    input: Option<&str>
) -> Result<Vec<crate::network::config::BootstrapPeer>, RuntimeError> {
    let Some(input) = input.map(str::trim).filter(|input| !input.is_empty()) else {
        return Ok(Vec::new());
    };

    input
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(parse_bootstrap_peer)
        .collect()
}

fn parse_bootstrap_peer(
    entry: &str
) -> Result<crate::network::config::BootstrapPeer, RuntimeError> {
    let (address, expected_transport_peer) = match entry.split_once('@') {
        Some((address, peer_id)) => (address, Some(parse_transport_peer_id(peer_id.trim())?)),
        None => (entry, None),
    };

    Ok(crate::network::config::BootstrapPeer {
        address: parse_bootstrap_address(address.trim())?,
        expected_transport_peer,
    })
}

fn parse_bootstrap_address(
    value: &str
) -> Result<crate::network::config::MultiaddrLite, RuntimeError> {
    let multiaddr = value
        .parse::<Multiaddr>()
        .map_err(|_| RuntimeError::config("BOOTSTRAP_PEERS contains an invalid multiaddr"))?;
    let mut protocols = multiaddr.iter();

    match (protocols.next(), protocols.next(), protocols.next()) {
        (Some(Protocol::Ip4(addr)), Some(Protocol::Tcp(port)), None) => {
            Ok(crate::network::config::MultiaddrLite::Ip4Tcp {
                addr: addr.octets(),
                port,
            })
        }
        (Some(Protocol::Dns4(host)), Some(Protocol::Tcp(port)), None) => {
            Ok(crate::network::config::MultiaddrLite::Dns4Tcp {
                host: host.into_owned(),
                port,
            })
        }
        _ =>
            Err(
                RuntimeError::config(
                    "BOOTSTRAP_PEERS only supports /ip4/.../tcp/... and /dns4/.../tcp/... entries"
                )
            ),
    }
}

fn parse_transport_peer_id(value: &str) -> Result<Vec<u8>, RuntimeError> {
    value
        .parse::<Libp2pPeerId>()
        .map(|peer_id| peer_id.to_bytes())
        .map_err(|_| RuntimeError::config("BOOTSTRAP_PEERS contains an invalid libp2p peer id"))
}
