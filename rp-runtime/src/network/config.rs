use std::time::Duration;

use libp2p::Multiaddr;

use crate::runtime::errors::RuntimeError;

pub const DEFAULT_MAX_FRAME_LEN: u32 = 64 * 1024;
pub const DEFAULT_MAX_BLOCKS_PER_CHUNK: u16 = 8;

const DEFAULT_LISTEN_ADDR: &str = "/ip4/0.0.0.0/tcp/4001";
const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 10;
const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 300;

#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub listen_addr: Multiaddr,
    pub bootstrap_addrs: Vec<Multiaddr>,
    pub request_timeout: Duration,
    pub idle_timeout: Duration,
    pub max_frame_len: u32,
    pub max_blocks_per_chunk: u16,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen_addr: DEFAULT_LISTEN_ADDR.parse().expect(
                "default listen multiaddr should parse"
            ),
            bootstrap_addrs: Vec::new(),
            request_timeout: Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS),
            idle_timeout: Duration::from_secs(DEFAULT_IDLE_TIMEOUT_SECS),
            max_frame_len: DEFAULT_MAX_FRAME_LEN,
            max_blocks_per_chunk: DEFAULT_MAX_BLOCKS_PER_CHUNK,
        }
    }
}

impl NetworkConfig {
    pub fn from_env() -> Result<Self, RuntimeError> {
        let mut config = Self::default();

        if let Ok(value) = std::env::var("RP_RUNTIME_LISTEN_ADDR") {
            config.listen_addr = value
                .parse::<Multiaddr>()
                .map_err(|_| RuntimeError::config("invalid RP_RUNTIME_LISTEN_ADDR"))?;
        }

        if let Ok(value) = std::env::var("RP_RUNTIME_BOOTSTRAP_PEERS") {
            config.bootstrap_addrs = value
                .split(',')
                .filter_map(|entry| {
                    let trimmed = entry.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(
                            trimmed
                                .parse::<Multiaddr>()
                                .map_err(|_| {
                                    RuntimeError::config("invalid RP_RUNTIME_BOOTSTRAP_PEERS")
                                })
                        )
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
        }

        if
            let Some(value) = std::env
                ::var("RP_RUNTIME_REQUEST_TIMEOUT_SECS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
        {
            config.request_timeout = Duration::from_secs(value);
        }

        Ok(config)
    }
}
