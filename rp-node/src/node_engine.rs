use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use rp_core::models::block::Block;
use rp_core::state::State;
use rp_core::traits::{ FromBytes, Hashable, ToBytes };

use crate::contract::BlockHash;
use crate::network::message::{ NetworkMessage, SyncResponse };
use crate::{ blockchain::Blockchain, contract::{ NodeAction, NodeInput, PeerId } };

pub struct ParkedBlock {
    pub peer: PeerId,
    pub block: Block,
}

pub struct NodeEngine {
    blockchain: Blockchain,
    pub peers: BTreeMap<PeerId, PeerState>,
    pub pending_requests: Vec<PendingRequest>,
    pub pending_blocks: BTreeMap<BlockHash, ParkedBlock>,
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
            pending_blocks: BTreeMap::new(),
        }
    }

    fn request_parent_block(&self, parked_block: &ParkedBlock) -> Vec<NodeAction> {
        let parent_height = parked_block.block.height.saturating_sub(1);

        vec![
            NodeAction::ReportEvent {
                message: "parent snapshot missing",
            },
            NodeAction::RequestBlocks {
                peer: parked_block.peer,
                from_height: parent_height,
                to_height: parent_height,
            }
        ]
    }

    fn resume_parked_child(&mut self, parent_hash: BlockHash) -> Vec<NodeAction> {
        let Some(parked_block) = self.pending_blocks.remove(&parent_hash) else {
            return Vec::new();
        };

        let parent_state = self.blockchain.state.clone();
        if self.blockchain.add_block(parked_block.block.clone(), &parent_state).is_err() {
            return vec![NodeAction::ReportEvent {
                message: "failed importing parked block",
            }];
        }

        let block_hash = parked_block.block.hash();
        let mut actions = vec![
            NodeAction::PersistBlock {
                block: parked_block.block,
            },
            NodeAction::PersistSnapshot {
                block_hash,
                state_bytes: self.blockchain.state.clone().to_bytes(),
            }
        ];

        if self.blockchain.head_hash == block_hash {
            actions.extend(self.resume_parked_child(block_hash));
        }

        actions
    }

    fn ingest_block(&mut self, peer: PeerId, block: Block) -> Vec<NodeAction> {
        if self.blockchain.head_hash == block.previous_hash {
            let parent_state = self.blockchain.state.clone();
            if self.blockchain.add_block(block.clone(), &parent_state).is_err() {
                return vec![NodeAction::ReportEvent {
                    message: "failed importing block",
                }];
            }

            let block_hash = block.hash();
            let mut actions = vec![
                NodeAction::PersistBlock {
                    block,
                },
                NodeAction::PersistSnapshot {
                    block_hash,
                    state_bytes: self.blockchain.state.clone().to_bytes(),
                }
            ];

            if self.blockchain.head_hash == block_hash {
                actions.extend(self.resume_parked_child(block_hash));
            }

            actions
        } else {
            self.pending_blocks.insert(block.previous_hash, ParkedBlock {
                peer,
                block: block.clone(),
            });
            vec![NodeAction::LoadSnapshot {
                block_hash: block.previous_hash,
            }]
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
                    NetworkMessage::NewBlock(block) => self.ingest_block(peer, block),
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
                        let mut actions = Vec::new();

                        self.pending_requests.retain(|request| request.peer != peer);

                        for block in response.blocks {
                            actions.extend(self.ingest_block(peer, block));
                        }

                        if actions.is_empty() {
                            vec![NodeAction::FrameReceived { peer }]
                        } else {
                            actions
                        }
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

            NodeInput::StorageLoaded { block_hash, state_bytes } => {
                let Some(parked_block) = self.pending_blocks.get(&block_hash) else {
                    return vec![NodeAction::ReportEvent {
                        message: "unexpected snapshot result",
                    }];
                };

                let Some(state_bytes) = state_bytes else {
                    return self.request_parent_block(parked_block);
                };

                let loaded_state = match State::from_bytes(&state_bytes) {
                    Ok(state) => state,
                    Err(_) => {
                        return vec![NodeAction::ReportEvent {
                            message: "invalid snapshot bytes",
                        }];
                    }
                };

                let Some(parked_block) = self.pending_blocks.remove(&block_hash) else {
                    return vec![NodeAction::ReportEvent {
                        message: "parked block disappeared",
                    }];
                };

                if self.blockchain.add_block(parked_block.block.clone(), &loaded_state).is_err() {
                    return vec![NodeAction::ReportEvent {
                        message: "failed importing parked block",
                    }];
                }

                let block_hash = parked_block.block.hash();
                let mut actions = vec![
                    NodeAction::PersistBlock {
                        block: parked_block.block,
                    },
                    NodeAction::PersistSnapshot {
                        block_hash,
                        state_bytes: self.blockchain.state.clone().to_bytes(),
                    }
                ];

                if self.blockchain.head_hash == block_hash {
                    actions.extend(self.resume_parked_child(block_hash));
                }

                actions
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
