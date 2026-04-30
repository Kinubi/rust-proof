use std::net::SocketAddr;

use crate::{
	network::config::MultiaddrLite,
	runtime::errors::RuntimeError,
};

pub const IDENTIFY_PROTOCOL: &str = "/ipfs/id/1.0.0";

#[derive(Debug, Clone)]
pub struct IdentifyInfo {
	pub protocol_version: String,
	pub agent_version: String,
	pub listen_addrs: Vec<MultiaddrLite>,
	pub supported_protocols: Vec<String>,
	pub observed_addr: Option<SocketAddr>,
	pub transport_peer_id: Vec<u8>,
}

pub fn encode_identify(info: &IdentifyInfo) -> Result<Vec<u8>, RuntimeError> {
	let _ = info;
	todo!("implement identify encoding")
}

pub fn decode_identify(bytes: &[u8]) -> Result<IdentifyInfo, RuntimeError> {
	let _ = bytes;
	todo!("implement identify decoding")
}
