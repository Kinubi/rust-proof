use std::collections::BTreeMap;

use crate::runtime::errors::RuntimeError;

pub type SessionId = usize;

pub struct PeerSession {
	pub id: SessionId,
	pub node_peer_id: Option<[u8; 32]>,
	pub transport_peer_id: Vec<u8>,
	pub state: SessionState,
	pub max_frame_len: u32,
	pub max_blocks_per_chunk: u16,
	pub last_seen_ms: u64,
}

pub enum SessionState {
	TcpConnected,
	NoiseReady,
	YamuxReady,
	Identified,
	NodeReady,
	Closing,
}

pub struct PeerRegistry {
	sessions: Vec<Option<PeerSession>>,
	by_node_peer: BTreeMap<[u8; 32], SessionId>,
	max_peers: usize,
}

impl PeerRegistry {
	pub fn new(max_peers: usize) -> Self {
		let _ = max_peers;
		todo!("implement peer registry initialization")
	}

	pub fn alloc(&mut self, transport_peer_id: Vec<u8>) -> Result<SessionId, RuntimeError> {
		let _ = transport_peer_id;
		todo!("implement peer registry allocation")
	}

	pub fn get(&self, id: SessionId) -> Option<&PeerSession> {
		let _ = id;
		todo!("implement peer lookup")
	}

	pub fn get_mut(&mut self, id: SessionId) -> Option<&mut PeerSession> {
		let _ = id;
		todo!("implement mutable peer lookup")
	}

	pub fn register_node_peer(
		&mut self,
		id: SessionId,
		node_peer_id: [u8; 32]
	) -> Result<(), RuntimeError> {
		let _ = (id, node_peer_id);
		todo!("implement node peer registration")
	}

	pub fn session_for_node(&self, peer: &[u8; 32]) -> Option<SessionId> {
		let _ = peer;
		todo!("implement node peer to session lookup")
	}

	pub fn ready_sessions(&self) -> Vec<SessionId> {
		todo!("implement ready session listing")
	}

	pub fn remove(&mut self, id: SessionId) -> Option<PeerSession> {
		let _ = id;
		todo!("implement peer removal")
	}
}
