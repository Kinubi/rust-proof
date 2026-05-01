use std::{ collections::BTreeMap, net::SocketAddr };

use crate::runtime::errors::RuntimeError;

pub type SessionId = usize;

pub struct PeerSession {
    pub id: SessionId,
    pub is_outbound: bool,
    pub outbound_addr: Option<SocketAddr>,
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
        Self {
            sessions: Vec::with_capacity(max_peers),
            by_node_peer: BTreeMap::new(),
            max_peers,
        }
    }

    pub fn alloc(
        &mut self,
        transport_peer_id: Vec<u8>,
        is_outbound: bool,
        outbound_addr: Option<SocketAddr>
    ) -> Result<SessionId, RuntimeError> {
        if
            let Some((session_id, slot)) = self.sessions
                .iter_mut()
                .enumerate()
                .find(|(_, slot)| slot.is_none())
        {
            *slot = Some(PeerSession {
                id: session_id,
                is_outbound,
                outbound_addr,
                node_peer_id: None,
                transport_peer_id,
                state: SessionState::TcpConnected,
                max_frame_len: 0,
                max_blocks_per_chunk: 0,
                last_seen_ms: 0,
            });
            return Ok(session_id);
        }

        if self.sessions.len() >= self.max_peers {
            return Err(RuntimeError::config("peer registry is full"));
        }

        let session_id = self.sessions.len();
        self.sessions.push(
            Some(PeerSession {
                id: session_id,
                is_outbound,
                outbound_addr,
                node_peer_id: None,
                transport_peer_id,
                state: SessionState::TcpConnected,
                max_frame_len: 0,
                max_blocks_per_chunk: 0,
                last_seen_ms: 0,
            })
        );
        Ok(session_id)
    }

    pub fn get(&self, id: SessionId) -> Option<&PeerSession> {
        self.sessions.get(id).and_then(Option::as_ref)
    }

    pub fn get_mut(&mut self, id: SessionId) -> Option<&mut PeerSession> {
        self.sessions.get_mut(id).and_then(Option::as_mut)
    }

    pub fn register_node_peer(
        &mut self,
        id: SessionId,
        node_peer_id: [u8; 32]
    ) -> Result<(), RuntimeError> {
        if let Some(existing) = self.by_node_peer.get(&node_peer_id) {
            if *existing != id {
                return Err(
                    RuntimeError::config("node peer is already registered to another session")
                );
            }
        }

        let session = self.get_mut(id).ok_or_else(|| RuntimeError::config("unknown peer session"))?;
        session.node_peer_id = Some(node_peer_id);
        self.by_node_peer.insert(node_peer_id, id);
        Ok(())
    }

    pub fn session_for_node(&self, peer: &[u8; 32]) -> Option<SessionId> {
        self.by_node_peer.get(peer).copied()
    }

    pub fn ready_sessions(&self) -> Vec<SessionId> {
        self.sessions
            .iter()
            .enumerate()
            .filter_map(|(session_id, session)| {
                let session = session.as_ref()?;
                matches!(session.state, SessionState::NodeReady).then_some(session_id)
            })
            .collect()
    }

    pub fn outbound_session_count(&self) -> usize {
        self.sessions
            .iter()
            .filter_map(Option::as_ref)
            .filter(|session| session.is_outbound)
            .count()
    }

    pub fn has_outbound_session_for(&self, address: &SocketAddr) -> bool {
        self.sessions
            .iter()
            .filter_map(Option::as_ref)
            .any(|session| {
                session.outbound_addr.as_ref().is_some_and(|outbound_addr| outbound_addr == address)
            })
    }

    pub fn remove(&mut self, id: SessionId) -> Option<PeerSession> {
        let session = self.sessions.get_mut(id)?.take()?;
        if let Some(node_peer_id) = session.node_peer_id {
            self.by_node_peer.remove(&node_peer_id);
        }
        Some(session)
    }
}
