use serde::{ Deserialize, Serialize };
use rp_node::contract::Identity;

use crate::{
	network::transport_identity::TransportIdentity,
	runtime::errors::RuntimeError,
};

pub const NODE_HELLO_PROTOCOL: &str = "/rust-proof/node-hello/1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHello {
	pub version: u16,
	pub node_public_key: Vec<u8>,
	pub node_peer_id: [u8; 32],
	pub transport_peer_id: Vec<u8>,
	pub max_frame_len: u32,
	pub max_blocks_per_chunk: u16,
	pub capabilities: PeerCapabilities,
	pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHelloResponse {
	pub accepted: bool,
	pub remote: NodeHello,
	pub reject_reason: Option<NodeHelloRejectReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeHelloRejectReason {
	VersionMismatch,
	InvalidSignature,
	PeerIdMismatch,
	TransportBindingMismatch,
	UnsupportedRequiredProtocol,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerCapabilities {
	pub supports_sync_v1: bool,
	pub supports_announce_v1: bool,
	pub supports_ping: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeHelloTranscript<'a> {
	pub version: u16,
	pub node_peer_id: [u8; 32],
	pub transport_peer_id: &'a [u8],
	pub max_frame_len: u32,
	pub max_blocks_per_chunk: u16,
	pub capabilities: &'a PeerCapabilities,
}

#[derive(Debug, Clone)]
pub struct VerifiedPeer {
	pub node_peer_id: [u8; 32],
	pub node_public_key: Vec<u8>,
	pub transport_peer_id: Vec<u8>,
	pub max_frame_len: u32,
	pub max_blocks_per_chunk: u16,
	pub capabilities: PeerCapabilities,
}

pub struct NodeHelloBuilder<'a> {
	pub node_identity: &'a dyn Identity,
	pub transport_identity: &'a dyn TransportIdentity,
	pub max_frame_len: u32,
	pub max_blocks_per_chunk: u16,
	pub capabilities: PeerCapabilities,
}

impl<'a> NodeHelloBuilder<'a> {
	pub fn build(&self) -> Result<NodeHello, RuntimeError> {
		todo!("implement node hello construction")
	}
}

pub struct NodeHelloVerifier;

impl NodeHelloVerifier {
	pub fn verify(
		remote: &NodeHello,
		authenticated_transport_peer: &[u8]
	) -> Result<VerifiedPeer, RuntimeError> {
		let _ = (remote, authenticated_transport_peer);
		todo!("implement node hello verification")
	}
}
