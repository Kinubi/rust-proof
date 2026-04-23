use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use rp_core::models::block::Block;
use rp_core::models::transaction::{ Transaction, TransactionData };
use rp_core::state::State;
use rp_core::traits::{ FromBytes, Hashable, ToBytes };

use crate::contract::BlockHash;
use crate::network::message::{ NetworkMessage, SyncResponse };
use crate::{ blockchain::Blockchain, contract::{ NodeAction, NodeInput, PeerId } };

const REQUEST_TIMEOUT_MS: u64 = 30_000;

pub struct ParkedBlock {
    pub peer: PeerId,
    pub block: Block,
}

pub struct NodeEngine {
    blockchain: Blockchain,
    pub peers: BTreeMap<PeerId, PeerState>,
    pub pending_requests: Vec<PendingRequest>,
    pub pending_blocks: BTreeMap<BlockHash, Vec<ParkedBlock>>,
    current_time_ms: u64,
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
    pub deadline_ms: u64,
}

impl NodeEngine {
    pub fn new(blockchain: Blockchain) -> Self {
        Self {
            blockchain,
            peers: BTreeMap::new(),
            pending_requests: Vec::new(),
            pending_blocks: BTreeMap::new(),
            current_time_ms: 0,
        }
    }

    fn prune_expired_requests(&mut self, now_ms: u64) {
        self.pending_requests.retain(|request| request.deadline_ms > now_ms);
    }

    fn relay_transaction(&self, source_peer: PeerId, transaction: &Transaction) -> Vec<NodeAction> {
        let frame = NetworkMessage::NewTransaction(transaction.clone()).to_bytes();

        self.peers
            .iter()
            .filter(|(peer, state)| **peer != source_peer && state.connected)
            .map(|(peer, _)| NodeAction::SendFrame {
                peer: *peer,
                frame: frame.clone(),
            })
            .collect()
    }

    fn request_parent_block(&self, parked_blocks: &[ParkedBlock]) -> Vec<NodeAction> {
        let Some(parked_block) = parked_blocks.first() else {
            return vec![NodeAction::ReportEvent {
                message: "no parked blocks for missing parent",
            }];
        };

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

    fn import_parked_blocks(
        &mut self,
        parent_hash: BlockHash,
        parent_state: &State
    ) -> Vec<NodeAction> {
        let Some(parked_blocks) = self.pending_blocks.remove(&parent_hash) else {
            return Vec::new();
        };

        let mut actions = Vec::new();

        for parked_block in parked_blocks {
            let imported_block = match
                self.blockchain.add_block(parked_block.block.clone(), parent_state)
            {
                Ok(imported_block) => imported_block,
                Err(_) => {
                    actions.push(NodeAction::ReportEvent {
                        message: "failed importing parked block",
                    });
                    continue;
                }
            };

            let block_hash = parked_block.block.hash();
            actions.push(NodeAction::PersistBlock {
                block: parked_block.block,
            });
            actions.push(NodeAction::PersistSnapshot {
                block_hash,
                state_bytes: imported_block.next_state.to_bytes(),
            });

            if imported_block.became_head {
                actions.extend(self.import_parked_blocks(block_hash, &imported_block.next_state));
            }
        }

        actions
    }

    fn ingest_block(&mut self, peer: PeerId, block: Block) -> Vec<NodeAction> {
        if self.blockchain.head_hash == block.previous_hash {
            let parent_state = self.blockchain.state.clone();
            let imported_block = match self.blockchain.add_block(block.clone(), &parent_state) {
                Ok(imported_block) => imported_block,
                Err(_) => {
                    return vec![NodeAction::ReportEvent {
                        message: "failed importing block",
                    }];
                }
            };

            let block_hash = block.hash();
            let mut actions = vec![
                NodeAction::PersistBlock {
                    block,
                },
                NodeAction::PersistSnapshot {
                    block_hash,
                    state_bytes: imported_block.next_state.to_bytes(),
                }
            ];

            if imported_block.became_head {
                actions.extend(self.import_parked_blocks(block_hash, &imported_block.next_state));
            }

            actions
        } else {
            self.pending_blocks.entry(block.previous_hash).or_default().push(ParkedBlock {
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
                self.current_time_ms = now_ms;
                self.prune_expired_requests(now_ms);

                let mut actions = Vec::new();
                actions.push(NodeAction::ScheduleWake { at_ms: now_ms + 1_000 });
                actions.push(NodeAction::ReportEvent { message: "We have a tick" });
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
                self.pending_requests.retain(|request| request.peer != peer);
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
                        match self.blockchain.add_transaction(tx.clone()) {
                            Ok(true) => self.relay_transaction(peer, &tx),
                            Ok(false) => Vec::new(),
                            Err(error) => vec![NodeAction::ReportEvent { message: error }],
                        }
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
                match self.blockchain.add_transaction(transaction.clone()) {
                    Ok(true) =>
                        vec![NodeAction::BroadcastFrame {
                            frame: NetworkMessage::NewTransaction(transaction).to_bytes(),
                        }],
                    Ok(false) => Vec::new(),
                    Err(_) =>
                        vec![NodeAction::ReportEvent {
                            message: "invalid transaction",
                        }],
                }
            }

            NodeInput::StorageLoaded { block_hash, state_bytes } => {
                let Some(parked_blocks) = self.pending_blocks.get(&block_hash) else {
                    return vec![NodeAction::ReportEvent {
                        message: "unexpected snapshot result",
                    }];
                };

                let Some(state_bytes) = state_bytes else {
                    return self.request_parent_block(parked_blocks);
                };

                let loaded_state = match State::from_bytes(&state_bytes) {
                    Ok(state) => state,
                    Err(_) => {
                        return vec![NodeAction::ReportEvent {
                            message: "invalid snapshot bytes",
                        }];
                    }
                };

                self.import_parked_blocks(block_hash, &loaded_state)
            }

            NodeInput::PersistCompleted { persist_type } => {
                vec![NodeAction::PersistCompleted { persist_type }]
            }

            NodeInput::ImportRequested { peer, from_height, to_height } => {
                self.pending_requests.push(PendingRequest {
                    peer,
                    from_height,
                    to_height,
                    deadline_ms: self.current_time_ms.saturating_add(REQUEST_TIMEOUT_MS),
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
    use rp_core::models::transaction::Transaction;
    use rp_core::traits::{ Hashable, ToBytes };

    use crate::contract::{ NodeAction, NodeInput };
    use crate::network::message::{ NetworkMessage, SyncResponse };

    fn build_transfer_transaction(
        sender: &SigningKey,
        receiver: &SigningKey,
        amount: u64,
        fee: u64,
        sequence: u64
    ) -> Transaction {
        let mut transaction = Transaction {
            sender: sender.verifying_key(),
            data: TransactionData::Transfer {
                receiver: receiver.verifying_key(),
                amount,
            },
            sequence,
            fee,
            signature: None,
        };
        let hash = transaction.hash();
        transaction.signature = Some(sender.sign(&hash));
        transaction
    }

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
            3
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
            3
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
            3
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
            NodeAction::RequestBlocks { peer: action_peer, from_height, to_height } => {
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

    #[test]
    fn test_peer_transaction_relays_to_other_connected_peers() {
        let mut csprng = OsRng;
        let sender = SigningKey::generate(&mut csprng);
        let receiver = SigningKey::generate(&mut csprng);
        let source_peer = [8u8; 32];
        let peer_a = [9u8; 32];
        let peer_b = [10u8; 32];

        let mut engine = NodeEngine::new(Blockchain::new().unwrap());
        engine.blockchain.state.balances.insert(sender.verifying_key().to_bytes(), 100);
        engine.step(NodeInput::PeerConnected { peer: source_peer });
        engine.step(NodeInput::PeerConnected { peer: peer_a });
        engine.step(NodeInput::PeerConnected { peer: peer_b });

        let transaction = build_transfer_transaction(&sender, &receiver, 25, 1, 0);
        let expected_frame = NetworkMessage::NewTransaction(transaction.clone()).to_bytes();

        let actions = engine.step(NodeInput::FrameReceived {
            peer: source_peer,
            frame: expected_frame.clone(),
        });

        assert_eq!(actions.len(), 2);
        match &actions[0] {
            NodeAction::SendFrame { peer, frame } => {
                assert_eq!(*peer, peer_a);
                assert_eq!(*frame, expected_frame);
            }
            _ => panic!("expected SendFrame action"),
        }
        match &actions[1] {
            NodeAction::SendFrame { peer, frame } => {
                assert_eq!(*peer, peer_b);
                assert_eq!(*frame, expected_frame);
            }
            _ => panic!("expected SendFrame action"),
        }
    }

    #[test]
    fn test_duplicate_peer_transaction_is_not_relayed() {
        let mut csprng = OsRng;
        let sender = SigningKey::generate(&mut csprng);
        let receiver = SigningKey::generate(&mut csprng);
        let source_peer = [11u8; 32];
        let other_peer = [12u8; 32];

        let mut engine = NodeEngine::new(Blockchain::new().unwrap());
        engine.blockchain.state.balances.insert(sender.verifying_key().to_bytes(), 100);
        engine.step(NodeInput::PeerConnected { peer: source_peer });
        engine.step(NodeInput::PeerConnected { peer: other_peer });

        let transaction = build_transfer_transaction(&sender, &receiver, 25, 1, 0);
        let frame = NetworkMessage::NewTransaction(transaction).to_bytes();

        let first_actions = engine.step(NodeInput::FrameReceived {
            peer: source_peer,
            frame: frame.clone(),
        });
        assert_eq!(first_actions.len(), 1);
        assert!(matches!(first_actions[0], NodeAction::SendFrame { .. }));

        let duplicate_actions = engine.step(NodeInput::FrameReceived {
            peer: source_peer,
            frame,
        });
        assert!(duplicate_actions.is_empty());
    }

    #[test]
    fn test_peer_disconnected_clears_pending_requests_for_peer() {
        let peer = [4u8; 32];
        let other_peer = [5u8; 32];

        let mut engine = NodeEngine::new(Blockchain::new().unwrap());
        engine.step(NodeInput::ImportRequested {
            peer,
            from_height: 1,
            to_height: 2,
        });
        engine.step(NodeInput::ImportRequested {
            peer: other_peer,
            from_height: 3,
            to_height: 4,
        });

        let actions = engine.step(NodeInput::PeerDisconnected { peer });

        assert_eq!(actions.len(), 1);
        assert_eq!(engine.pending_requests.len(), 1);
        assert_eq!(engine.pending_requests[0].peer, other_peer);
    }

    #[test]
    fn test_tick_prunes_expired_pending_requests() {
        let peer = [6u8; 32];

        let mut engine = NodeEngine::new(Blockchain::new().unwrap());
        engine.step(NodeInput::Tick { now_ms: 1_000 });
        engine.step(NodeInput::ImportRequested {
            peer,
            from_height: 1,
            to_height: 1,
        });

        assert_eq!(engine.pending_requests.len(), 1);
        assert_eq!(engine.pending_requests[0].deadline_ms, 31_000);

        let actions = engine.step(NodeInput::Tick { now_ms: 31_000 });

        assert_eq!(actions.len(), 1);
        assert!(engine.pending_requests.is_empty());
    }

    #[test]
    fn test_storage_loaded_imports_multiple_parked_siblings() {
        let mut csprng = OsRng;
        let validator_a = SigningKey::generate(&mut csprng);
        let validator_b = SigningKey::generate(&mut csprng);
        let (mut engine, peer, block_a, parent_state_bytes) = setup_forked_engine();

        let child_a = build_empty_block(
            &block_a,
            &State::from_bytes(&parent_state_bytes).unwrap(),
            &validator_a,
            3
        );
        let child_b = build_empty_block(
            &block_a,
            &State::from_bytes(&parent_state_bytes).unwrap(),
            &validator_b,
            4
        );

        engine.step(NodeInput::FrameReceived {
            peer,
            frame: NetworkMessage::NewBlock(child_a.clone()).to_bytes(),
        });
        engine.step(NodeInput::FrameReceived {
            peer,
            frame: NetworkMessage::NewBlock(child_b.clone()).to_bytes(),
        });

        assert_eq!(engine.pending_blocks.get(&block_a.hash()).unwrap().len(), 2);

        let actions = engine.step(NodeInput::StorageLoaded {
            block_hash: block_a.hash(),
            state_bytes: Some(parent_state_bytes),
        });

        assert_eq!(actions.len(), 4);
        assert!(!engine.pending_blocks.contains_key(&block_a.hash()));
        assert_eq!(engine.blockchain.head_hash, child_b.hash());
    }
}
