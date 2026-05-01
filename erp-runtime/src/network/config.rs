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
    Ip4Tcp { addr: [u8; 4], port: u16 },
    Dns4Tcp { host: String, port: u16 },
}

#[derive(Debug, Clone)]
pub struct BootstrapPeer {
    pub address: MultiaddrLite,
    pub expected_transport_peer: Option<Vec<u8>>,
}
