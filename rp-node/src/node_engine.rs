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

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{ Signer, SigningKey };
    use rand::rngs::OsRng;
    use rp_core::traits::{ Hashable, ToBytes };

    use crate::contract::{ NodeAction, NodeInput };
    use crate::network::message::{ NetworkMessage, SyncResponse };

    fn build_empty_block(
        parent: &Block,
        parent_state: &State,
        validator: &SigningKey,
        slot: u64
    ) -> Block {
        let mut block = Block {
            height: parent.height + 1,
            slot,
            previous_hash: parent.hash(),
            validator: validator.verifying_key(),
            transactions: vec![],
            signature: None,
            slash_proofs: vec![],
            state_root: parent_state.compute_state_root(),
        };
        let hash = block.hash();
        block.signature = Some(validator.sign(&hash));
        block
    }

    fn assert_persist_actions(actions: &[NodeAction], expected_hash: [u8; 32]) {
        assert_eq!(actions.len(), 2);

        match &actions[0] {
            NodeAction::PersistBlock { block } => {
                assert_eq!(block.hash(), expected_hash);
            }
            _ => panic!("expected PersistBlock action"),
        }

        match &actions[1] {
            NodeAction::PersistSnapshot { block_hash, .. } => {
                assert_eq!(*block_hash, expected_hash);
            }
            _ => panic!("expected PersistSnapshot action"),
        }
    }

    fn setup_forked_engine() -> (NodeEngine, PeerId, Block, Vec<u8>) {
        let mut csprng = OsRng;
        let validator_a = SigningKey::generate(&mut csprng);
        let validator_b = SigningKey::generate(&mut csprng);
        let peer = [7u8; 32];

        let mut engine = NodeEngine::new(Blockchain::new().unwrap());
        let genesis = engine.blockchain.get_latest_block().clone();
        let parent_state_bytes = engine.blockchain.state.to_bytes();

        let block_a = build_empty_block(&genesis, &engine.blockchain.state, &validator_a, 1);
        let actions = engine.step(NodeInput::FrameReceived {
            peer,
            frame: NetworkMessage::NewBlock(block_a.clone()).to_bytes(),
        });
        assert_persist_actions(&actions, block_a.hash());

        let block_b = build_empty_block(&genesis, &engine.blockchain.state, &validator_b, 2);
        let actions = engine.step(NodeInput::FrameReceived {
            peer,
            frame: NetworkMessage::NewBlock(block_b).to_bytes(),
        });
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            NodeAction::LoadSnapshot { block_hash } => {
                assert_eq!(*block_hash, genesis.hash());
            }
            _ => panic!("expected LoadSnapshot action"),
        }

        let actions = engine.step(NodeInput::StorageLoaded {
            block_hash: genesis.hash(),
            state_bytes: Some(parent_state_bytes.clone()),
        });
        assert_eq!(engine.blockchain.get_latest_block().slot, 2);
        assert_persist_actions(&actions, engine.blockchain.head_hash);

        (engine, peer, block_a, parent_state_bytes)
    }

    #[test]
    fn test_frame_received_direct_child_persists_block_and_snapshot() {
        let mut csprng = OsRng;
        let validator = SigningKey::generate(&mut csprng);
        let peer = [1u8; 32];

        let mut engine = NodeEngine::new(Blockchain::new().unwrap());
        let genesis = engine.blockchain.get_latest_block().clone();
        let block = build_empty_block(&genesis, &engine.blockchain.state, &validator, 1);

        let actions = engine.step(NodeInput::FrameReceived {
            peer,
            frame: NetworkMessage::NewBlock(block.clone()).to_bytes(),
        });

        assert_eq!(engine.blockchain.head_hash, block.hash());
        assert!(engine.pending_blocks.is_empty());
        assert_persist_actions(&actions, block.hash());
    }

    #[test]
    fn test_frame_received_non_head_child_emits_load_snapshot() {
        let mut csprng = OsRng;
        let validator = SigningKey::generate(&mut csprng);
        let (mut engine, peer, block_a, parent_state_bytes) = setup_forked_engine();
        let child_of_a = build_empty_block(
            &block_a,
            &State::from_bytes(&parent_state_bytes).unwrap(),
            &validator,
            3,
        );

        let actions = engine.step(NodeInput::FrameReceived {
            peer,
            frame: NetworkMessage::NewBlock(child_of_a.clone()).to_bytes(),
        });

        assert_eq!(actions.len(), 1);
        match &actions[0] {
            NodeAction::LoadSnapshot { block_hash } => {
                assert_eq!(*block_hash, block_a.hash());
            }
            _ => panic!("expected LoadSnapshot action"),
        }
        assert!(engine.pending_blocks.contains_key(&block_a.hash()));
    }

    #[test]
    fn test_storage_loaded_imports_parked_block_and_persists_it() {
        let mut csprng = OsRng;
        let validator = SigningKey::generate(&mut csprng);
        let (mut engine, peer, block_a, parent_state_bytes) = setup_forked_engine();
        let child_of_a = build_empty_block(
            &block_a,
            &State::from_bytes(&parent_state_bytes).unwrap(),
            &validator,
            3,
        );

        let actions = engine.step(NodeInput::FrameReceived {
            peer,
            frame: NetworkMessage::NewBlock(child_of_a.clone()).to_bytes(),
        });
        assert!(matches!(&actions[0], NodeAction::LoadSnapshot { .. }));

        let actions = engine.step(NodeInput::StorageLoaded {
            block_hash: block_a.hash(),
            state_bytes: Some(parent_state_bytes),
        });

        assert_eq!(engine.blockchain.head_hash, child_of_a.hash());
        assert!(!engine.pending_blocks.contains_key(&block_a.hash()));
        assert_persist_actions(&actions, child_of_a.hash());
    }

    #[test]
    fn test_storage_loaded_none_requests_parent_block() {
        let mut csprng = OsRng;
        let validator = SigningKey::generate(&mut csprng);
        let (mut engine, peer, block_a, parent_state_bytes) = setup_forked_engine();
        let child_of_a = build_empty_block(
            &block_a,
            &State::from_bytes(&parent_state_bytes).unwrap(),
            &validator,
            3,
        );

        let actions = engine.step(NodeInput::FrameReceived {
            peer,
            frame: NetworkMessage::NewBlock(child_of_a).to_bytes(),
        });
        assert!(matches!(&actions[0], NodeAction::LoadSnapshot { .. }));

        let actions = engine.step(NodeInput::StorageLoaded {
            block_hash: block_a.hash(),
            state_bytes: None,
        });

        assert_eq!(actions.len(), 2);
        match &actions[0] {
            NodeAction::ReportEvent { message } => {
                assert_eq!(*message, "parent snapshot missing");
            }
            _ => panic!("expected ReportEvent action"),
        }
        match &actions[1] {
            NodeAction::RequestBlocks {
                peer: action_peer,
                from_height,
                to_height,
            } => {
                assert_eq!(*action_peer, peer);
                assert_eq!(*from_height, 1);
                assert_eq!(*to_height, 1);
            }
            _ => panic!("expected RequestBlocks action"),
        }
    }

    #[test]
    fn test_sync_response_uses_ingest_path_and_clears_pending_request() {
        let mut csprng = OsRng;
        let validator = SigningKey::generate(&mut csprng);
        let peer = [3u8; 32];

        let mut engine = NodeEngine::new(Blockchain::new().unwrap());
        let genesis = engine.blockchain.get_latest_block().clone();
        let block = build_empty_block(&genesis, &engine.blockchain.state, &validator, 1);

        let request_actions = engine.step(NodeInput::ImportRequested {
            peer,
            from_height: 1,
            to_height: 1,
        });
        assert_eq!(engine.pending_requests.len(), 1);
        assert!(matches!(&request_actions[0], NodeAction::RequestBlocks { .. }));

        let actions = engine.step(NodeInput::FrameReceived {
            peer,
            frame: NetworkMessage::SyncResponse(SyncResponse {
                blocks: vec![block.clone()],
            }).to_bytes(),
        });

        assert!(engine.pending_requests.is_empty());
        assert_eq!(engine.blockchain.head_hash, block.hash());
        assert_persist_actions(&actions, block.hash());
    }
}
