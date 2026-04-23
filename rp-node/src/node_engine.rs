
use alloc::vec::Vec;
use alloc::vec;
use alloc::collections::BTreeMap;
use crate::{ blockchain::Blockchain, contract::{ NodeAction, NodeInput, PeerId } };

pub struct NodeEngine {
    blockchain: Blockchain,
    pub peers: BTreeMap<PeerId, PeerState>,
    pub pending_requests: Vec<PendingRequest>,
}

#[derive(Debug, Clone)]
pub struct PeerState {
    pub connected: bool,
    pub last_seen_ms: u64,
}

#[derive(Debug, Clone)]
pub struct PendingRequest {
    pub peer: PeerId,
    pub from_height: u64,
    pub to_height: u64,
}

impl NodeEngine {
    pub fn new(blockchain: Blockchain) -> Self {
        Self {
            blockchain,
            peers: BTreeMap::new(),
            pending_requests: Vec::new(),
        }
    }

    pub fn step(&mut self, input: NodeInput) -> Vec<NodeAction> {
        match input {
            NodeInput::Tick { now_ms } => {
                let mut actions = Vec::new();
                actions.push(NodeAction::ScheduleWake { at_ms: now_ms + 1_000 });
                actions
            }

            NodeInput::PeerConnected { peer } => {
                self.peers.insert(peer, PeerState {
                    connected: true,
                    last_seen_ms: 0,
                });
                vec![NodeAction::ReportEvent {
                    message: "peer connected",
                }]
            }

            NodeInput::PeerDisconnected { peer } => {
                self.peers.remove(&peer);
                vec![NodeAction::ReportEvent {
                    message: "peer disconnected",
                }]
            }

            NodeInput::FrameReceived { peer, frame: _frame } => {
                vec![NodeAction::FrameReceived { peer }]
            }

            NodeInput::LocalTransactionSubmitted { transaction } => {
                let _ = self.blockchain.add_transaction(transaction);
                vec![NodeAction::BroadcastFrame {
                    frame: Vec::new(),
                }]
            }

            NodeInput::StorageLoaded { block_hash: _, state_bytes: _ } => {
                vec![NodeAction::ReportEvent {
                    message: "storage loaded",
                }]
            }

            NodeInput::PersistCompleted { persist_type } => {
                vec![NodeAction::PersistCompleted { persist_type }]
            }

            NodeInput::ImportRequested { peer, from_height, to_height } => {
                self.pending_requests.push(PendingRequest {
                    peer,
                    from_height,
                    to_height,
                });
                vec![NodeAction::RequestBlocks {
                    peer,
                    from_height,
                    to_height,
                }]
            }
        }
    }
}
