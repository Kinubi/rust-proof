use libp2p_identity::PeerId as Libp2pPeerId;
use multiaddr::{ Multiaddr, Protocol };

use crate::runtime::errors::RuntimeError;

pub const DEFAULT_LISTEN_PORT: u16 = 4001;
pub const DEFAULT_MAX_PEERS: usize = 16;
pub const DEFAULT_MAX_OUTBOUND_DIALS: usize = 4;
pub const DEFAULT_MAX_FRAME_LEN: u32 = 64 * 1024;
pub const DEFAULT_MAX_BLOCKS_PER_CHUNK: u16 = 8;
pub const DEFAULT_IDLE_TIMEOUT_MS: u64 = 60_000;

const BOOTSTRAP_PEERS_ENV: Option<&str> = option_env!("BOOTSTRAP_PEERS");

#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub listen_port: u16,
    pub max_peers: usize,
    pub max_outbound_dials: usize,
    pub max_frame_len: u32,
    pub max_blocks_per_chunk: u16,
    pub idle_timeout_ms: u64,
    pub bootstrap_peers: Vec<BootstrapPeer>,
}

#[derive(Debug, Clone)]
pub enum MultiaddrLite {
    Ip4Tcp {
        addr: [u8; 4],
        port: u16,
    },
    Dns4Tcp {
        host: String,
        port: u16,
    },
}

#[derive(Debug, Clone)]
pub struct BootstrapPeer {
    pub address: MultiaddrLite,
    pub expected_transport_peer: Option<Vec<u8>>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen_port: DEFAULT_LISTEN_PORT,
            max_peers: DEFAULT_MAX_PEERS,
            max_outbound_dials: DEFAULT_MAX_OUTBOUND_DIALS,
            max_frame_len: DEFAULT_MAX_FRAME_LEN,
            max_blocks_per_chunk: DEFAULT_MAX_BLOCKS_PER_CHUNK,
            idle_timeout_ms: DEFAULT_IDLE_TIMEOUT_MS,
            bootstrap_peers: Vec::new(),
        }
    }
}

impl NetworkConfig {
    pub fn from_build_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            bootstrap_peers: parse_bootstrap_peers(BOOTSTRAP_PEERS_ENV)?,
            ..Self::default()
        })
    }
}

fn parse_bootstrap_peers(input: Option<&str>) -> Result<Vec<BootstrapPeer>, RuntimeError> {
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

fn parse_bootstrap_peer(entry: &str) -> Result<BootstrapPeer, RuntimeError> {
    let (address, expected_transport_peer) = match entry.split_once('@') {
        Some((address, peer_id)) => (address, Some(parse_transport_peer_id(peer_id.trim())?)),
        None => (entry, None),
    };

    Ok(BootstrapPeer {
        address: parse_bootstrap_address(address.trim())?,
        expected_transport_peer,
    })
}

fn parse_bootstrap_address(value: &str) -> Result<MultiaddrLite, RuntimeError> {
    let multiaddr = value
        .parse::<Multiaddr>()
        .map_err(|_| RuntimeError::config("BOOTSTRAP_PEERS contains an invalid multiaddr"))?;
    let mut protocols = multiaddr.iter();

    match (protocols.next(), protocols.next(), protocols.next()) {
        (Some(Protocol::Ip4(addr)), Some(Protocol::Tcp(port)), None) => {
            Ok(MultiaddrLite::Ip4Tcp {
                addr: addr.octets(),
                port,
            })
        }
        (Some(Protocol::Dns4(host)), Some(Protocol::Tcp(port)), None) => {
            Ok(MultiaddrLite::Dns4Tcp {
                host: host.into_owned(),
                port,
            })
        }
        _ => {
            Err(
                RuntimeError::config(
                    "BOOTSTRAP_PEERS only supports /ip4/.../tcp/... and /dns4/.../tcp/... entries"
                )
            )
        }
    }
}

fn parse_transport_peer_id(value: &str) -> Result<Vec<u8>, RuntimeError> {
    value
        .parse::<Libp2pPeerId>()
        .map(|peer_id| peer_id.to_bytes())
        .map_err(|_| RuntimeError::config("BOOTSTRAP_PEERS contains an invalid libp2p peer id"))
}
