use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use rp_core::traits::{ FromBytes, ToBytes };

use crate::network::message::{ NetworkMessage, SyncResponse };
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

            NodeInput::FrameReceived { peer, frame } => {
                let message = match NetworkMessage::from_bytes(&frame) {
                    Ok(message) => message,
                    Err(_) => {
                        return vec![NodeAction::ReportEvent {
                            message: "invalid frame",
                        }];
                    }
                };

                match message {
                    NetworkMessage::NewBlock(block) => {
                        let parent_state = self.blockchain.state.clone();
                        if self.blockchain.add_block(block, &parent_state).is_err() {
                            return vec![NodeAction::ReportEvent {
                                message: "failed adding block",
                            }];
                        }
                        vec![NodeAction::FrameReceived { peer }]
                    }
                    NetworkMessage::NewTransaction(tx) => {
                        if let Err(error) = self.blockchain.add_transaction(tx) {
                            return vec![NodeAction::ReportEvent { message: error }];
                        }
                        vec![NodeAction::FrameReceived { peer }]
                    }
                    NetworkMessage::SyncRequest(request) => {
                        let blocks = self.blockchain.get_blocks(
                            request.from_height,
                            request.to_height
                        );
                        vec![NodeAction::SendFrame {
                            peer,
                            frame: NetworkMessage::SyncResponse(SyncResponse { blocks }).to_bytes(),
                        }]
                    }
                    NetworkMessage::SyncResponse(response) => {
                        for block in response.blocks {
                            let parent_state = self.blockchain.state.clone();
                            if self.blockchain.add_block(block, &parent_state).is_err() {
                                return vec![NodeAction::ReportEvent {
                                    message: "failed importing block",
                                }];
                            }
                        }
                        vec![NodeAction::FrameReceived { peer }]
                    }
                }
            }

            NodeInput::LocalTransactionSubmitted { transaction } => {
                if self.blockchain.add_transaction(transaction.clone()).is_ok() {
                    vec![NodeAction::BroadcastFrame {
                        frame: NetworkMessage::NewTransaction(transaction).to_bytes(),
                    }]
                } else {
                    vec![NodeAction::ReportEvent {
                        message: "invalid transaction",
                    }]
                }
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
