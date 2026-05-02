use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use rp_core::models::block::Block;
use rp_core::models::transaction::{ Transaction };
use rp_core::state::State;
use rp_core::traits::{ FromBytes, Hashable, ToBytes };

use crate::contract::BlockHash;
use crate::network::message::{ AnnounceKind, AnnounceRequest, NetworkMessage, SyncResponse };
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
    scheduled_wake_at_ms: Option<u64>,
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
            scheduled_wake_at_ms: None,
        }
    }

    pub fn restore_latest_snapshot(
        &mut self,
        block: Option<Block>,
        state_bytes: Option<Vec<u8>>
    ) -> Vec<NodeAction> {
        let (Some(block), Some(state_bytes)) = (block, state_bytes) else {
            return vec![NodeAction::ReportEvent {
                message: "no snapshot restored",
            }];
        };

        let restored_state = match State::from_bytes(&state_bytes) {
            Ok(state) => state,
            Err(_) => {
                return vec![NodeAction::ReportEvent {
                    message: "invalid startup snapshot bytes",
                }];
            }
        };

        if block.state_root != restored_state.compute_state_root() {
            return vec![NodeAction::ReportEvent {
                message: "startup snapshot state root mismatch",
            }];
        }

        self.blockchain.restore_head(block, restored_state);

        vec![NodeAction::ReportEvent {
            message: "startup snapshot restored",
        }]
    }

    fn prune_expired_requests(&mut self, now_ms: u64) {
        self.pending_requests.retain(|request| request.deadline_ms > now_ms);
    }

    fn sync_wake_action(&mut self) -> Option<NodeAction> {
        let next_wake_at_ms = self.pending_requests
            .iter()
            .map(|request| request.deadline_ms)
            .min();

        if next_wake_at_ms == self.scheduled_wake_at_ms {
            return None;
        }

        self.scheduled_wake_at_ms = next_wake_at_ms;

        match next_wake_at_ms {
            Some(at_ms) => Some(NodeAction::ScheduleWake { at_ms }),
            None => Some(NodeAction::CancelWake),
        }
    }

    fn relay_transaction(&self, source_peer: PeerId, transaction: &Transaction) -> Vec<NodeAction> {
        let frame = NetworkMessage::AnnounceRequest(
            AnnounceRequest::transaction(transaction.clone())
        ).to_bytes();

        self.peers
            .iter()
            .filter(|(peer, state)| **peer != source_peer && state.connected)
            .map(|(peer, _)| NodeAction::SendFrame {
                peer: *peer,
                frame: frame.clone(),
            })
            .collect()
    }

    fn queue_block_request(
        &mut self,
        peer: PeerId,
        from_height: u64,
        to_height: u64
    ) -> NodeAction {
        self.pending_requests.push(PendingRequest {
            peer,
            from_height,
            to_height,
            deadline_ms: self.current_time_ms.saturating_add(REQUEST_TIMEOUT_MS),
        });

        NodeAction::RequestBlocks {
            peer,
            from_height,
            to_height,
        }
    }

    fn request_parent_block(&mut self, peer: PeerId, parent_height: u64) -> Vec<NodeAction> {
        vec![
            NodeAction::ReportEvent {
                message: "parent snapshot missing",
            },
            self.queue_block_request(peer, parent_height, parent_height)
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

    fn continue_request(
        &mut self,
        request: &PendingRequest,
        next_height: u64
    ) -> Option<NodeAction> {
        if next_height == 0 {
            return None;
        }

        let next_to_height = if next_height <= request.to_height {
            request.to_height
        } else {
            let span = request.to_height.saturating_sub(request.from_height);
            next_height.saturating_add(span)
        };

        Some(self.queue_block_request(request.peer, next_height, next_to_height))
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
                },
                NodeAction::ReportEvent {
                    message: "Block added",
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
        let mut actions = match input {
            NodeInput::Tick { now_ms } => {
                self.current_time_ms = now_ms;
                self.prune_expired_requests(now_ms);

                Vec::new()
            }

            NodeInput::PeerConnected { peer } => {
                self.peers.insert(peer, PeerState {
                    connected: true,
                    last_seen_ms: 0,
                });

                // Request blocks from the new peer starting after our current head
                let local_height = self.blockchain.get_latest_block().height;
                let from_height = local_height.saturating_add(1);
                let to_height = local_height.saturating_add(100);
                vec![
                    NodeAction::ReportEvent {
                        message: "peer connected",
                    },
                    self.queue_block_request(peer, from_height, to_height)
                ]
            }

            NodeInput::PeerDisconnected { peer } => {
                self.peers.remove(&peer);
                self.pending_requests.retain(|request| request.peer != peer);
                vec![NodeAction::ReportEvent {
                    message: "peer disconnected",
                }]
            }

            NodeInput::FrameReceived { peer, frame } => {
                match NetworkMessage::from_bytes(&frame) {
                    Ok(message) =>
                        match message {
                            NetworkMessage::AnnounceRequest(request) =>
                                match request.kind {
                                    AnnounceKind::NewBlock(block) => self.ingest_block(peer, block),
                                    AnnounceKind::NewTransaction(tx) => {
                                        match self.blockchain.add_transaction(tx.clone()) {
                                            Ok(true) => self.relay_transaction(peer, &tx),
                                            Ok(false) => Vec::new(),
                                            Err(error) =>
                                                vec![NodeAction::ReportEvent { message: error }],
                                        }
                                    }
                                }
                            NetworkMessage::SyncRequest(request) => {
                                let blocks = self.blockchain.get_blocks(
                                    request.from_height,
                                    request.to_height
                                );
                                let earliest_contiguous_height =
                                    self.blockchain.earliest_contiguous_height();
                                let latest_height = self.blockchain.get_latest_block().height;

                                let highest_served_height = blocks.last().map(|block| block.height);
                                let has_more = highest_served_height
                                    .map(
                                        |height| height < self.blockchain.get_latest_block().height
                                    )
                                    .unwrap_or(false);

                                let next_height = if let Some(height) = highest_served_height {
                                    if has_more { Some(height.saturating_add(1)) } else { None }
                                } else if
                                    request.from_height < earliest_contiguous_height &&
                                    earliest_contiguous_height <= latest_height
                                {
                                    Some(earliest_contiguous_height)
                                } else {
                                    None
                                };

                                vec![NodeAction::SendFrame {
                                    peer,
                                    frame: NetworkMessage::SyncResponse(SyncResponse {
                                        blocks,
                                        has_more,
                                        next_height,
                                    }).to_bytes(),
                                }]
                            }
                            NetworkMessage::SyncResponse(response) => {
                                let mut actions = Vec::new();

                                let completed_requests = self.pending_requests
                                    .iter()
                                    .filter(|request| request.peer == peer)
                                    .cloned()
                                    .collect::<Vec<_>>();
                                self.pending_requests.retain(|request| request.peer != peer);

                                for block in response.blocks {
                                    actions.extend(self.ingest_block(peer, block));
                                }

                                if let Some(next_height) = response.next_height {
                                    for request in &completed_requests {
                                        if
                                            let Some(action) = self.continue_request(
                                                request,
                                                next_height
                                            )
                                        {
                                            actions.push(action);
                                        }
                                    }
                                }

                                if actions.is_empty() {
                                    vec![NodeAction::FrameReceived { peer }]
                                } else {
                                    actions
                                }
                            }
                            NetworkMessage::AnnounceResponse(_) =>
                                vec![NodeAction::FrameReceived { peer }],
                        }
                    Err(_) =>
                        vec![NodeAction::ReportEvent {
                            message: "invalid frame",
                        }],
                }
            }

            NodeInput::LocalTransactionSubmitted { transaction } => {
                match self.blockchain.add_transaction(transaction.clone()) {
                    Ok(true) =>
                        vec![NodeAction::BroadcastFrame {
                            frame: NetworkMessage::AnnounceRequest(
                                AnnounceRequest::transaction(transaction)
                            ).to_bytes(),
                        }],
                    Ok(false) => Vec::new(),
                    Err(_) =>
                        vec![NodeAction::ReportEvent {
                            message: "invalid transaction",
                        }],
                }
            }

            NodeInput::StorageLoaded { block_hash, state_bytes } => {
                if let Some((parent_peer, parent_height)) = self.pending_blocks
                    .get(&block_hash)
                    .and_then(|parked_blocks|
                        parked_blocks
                            .first()
                            .map(|parked_block| {
                                (parked_block.peer, parked_block.block.height.saturating_sub(1))
                            })
                    )
                {
                    if let Some(state_bytes) = state_bytes {
                        match State::from_bytes(&state_bytes) {
                            Ok(state) => self.import_parked_blocks(block_hash, &state),
                            Err(_) => vec![NodeAction::ReportEvent {
                                message: "invalid snapshot bytes",
                            }],
                        }
                    } else {
                        self.request_parent_block(parent_peer, parent_height)
                    }
                } else {
                    vec![NodeAction::ReportEvent {
                        message: "unexpected snapshot result",
                    }]
                }
            }

            NodeInput::PersistCompleted { persist_type } => {
                vec![NodeAction::PersistCompleted { persist_type }]
            }

            NodeInput::ImportRequested { peer, from_height, to_height } => {
                vec![self.queue_block_request(peer, from_height, to_height)]
            }
        };

        if let Some(action) = self.sync_wake_action() {
            actions.push(action);
        }

        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;
    use rp_core::crypto::{ Signer, SigningKey, verifying_key_to_bytes };
    use rp_core::models::transaction::{ Transaction, TransactionData };
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
            sender: sender.verifying_key().clone(),
            data: TransactionData::Transfer {
                receiver: receiver.verifying_key().clone(),
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
            validator: validator.verifying_key().clone(),
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
        assert!(actions.len() >= 2);

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
        let validator_a = SigningKey::random(&mut csprng);
        let validator_b = SigningKey::random(&mut csprng);
        let peer = [7u8; 32];

        let mut engine = NodeEngine::new(Blockchain::new().unwrap());
        let genesis = engine.blockchain.get_latest_block().clone();
        let parent_state_bytes = engine.blockchain.state.to_bytes();

        let block_a = build_empty_block(&genesis, &engine.blockchain.state, &validator_a, 1);
        let actions = engine.step(NodeInput::FrameReceived {
            peer,
            frame: NetworkMessage::AnnounceRequest(
                AnnounceRequest::block(block_a.clone())
            ).to_bytes(),
        });
        assert_persist_actions(&actions, block_a.hash());

        let block_b = build_empty_block(&genesis, &engine.blockchain.state, &validator_b, 2);
        let actions = engine.step(NodeInput::FrameReceived {
            peer,
            frame: NetworkMessage::AnnounceRequest(AnnounceRequest::block(block_b)).to_bytes(),
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
        let validator = SigningKey::random(&mut csprng);
        let peer = [1u8; 32];

        let mut engine = NodeEngine::new(Blockchain::new().unwrap());
        let genesis = engine.blockchain.get_latest_block().clone();
        let block = build_empty_block(&genesis, &engine.blockchain.state, &validator, 1);

        let actions = engine.step(NodeInput::FrameReceived {
            peer,
            frame: NetworkMessage::AnnounceRequest(
                AnnounceRequest::block(block.clone())
            ).to_bytes(),
        });

        assert_eq!(engine.blockchain.head_hash, block.hash());
        assert!(engine.pending_blocks.is_empty());
        assert_persist_actions(&actions, block.hash());
    }

    #[test]
    fn test_frame_received_non_head_child_emits_load_snapshot() {
        let mut csprng = OsRng;
        let validator = SigningKey::random(&mut csprng);
        let (mut engine, peer, block_a, parent_state_bytes) = setup_forked_engine();
        let child_of_a = build_empty_block(
            &block_a,
            &State::from_bytes(&parent_state_bytes).unwrap(),
            &validator,
            3
        );

        let actions = engine.step(NodeInput::FrameReceived {
            peer,
            frame: NetworkMessage::AnnounceRequest(
                AnnounceRequest::block(child_of_a.clone())
            ).to_bytes(),
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
        let validator = SigningKey::random(&mut csprng);
        let (mut engine, peer, block_a, parent_state_bytes) = setup_forked_engine();
        let child_of_a = build_empty_block(
            &block_a,
            &State::from_bytes(&parent_state_bytes).unwrap(),
            &validator,
            3
        );

        let actions = engine.step(NodeInput::FrameReceived {
            peer,
            frame: NetworkMessage::AnnounceRequest(
                AnnounceRequest::block(child_of_a.clone())
            ).to_bytes(),
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
        let validator = SigningKey::random(&mut csprng);
        let (mut engine, peer, block_a, parent_state_bytes) = setup_forked_engine();
        let child_of_a = build_empty_block(
            &block_a,
            &State::from_bytes(&parent_state_bytes).unwrap(),
            &validator,
            3
        );

        let actions = engine.step(NodeInput::FrameReceived {
            peer,
            frame: NetworkMessage::AnnounceRequest(AnnounceRequest::block(child_of_a)).to_bytes(),
        });
        assert!(matches!(&actions[0], NodeAction::LoadSnapshot { .. }));

        let actions = engine.step(NodeInput::StorageLoaded {
            block_hash: block_a.hash(),
            state_bytes: None,
        });

        assert_eq!(actions.len(), 3);
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
        assert!(matches!(actions[2], NodeAction::ScheduleWake { at_ms: 30_000 }));
    }

    #[test]
    fn test_sync_response_uses_ingest_path_and_clears_pending_request() {
        let mut csprng = OsRng;
        let validator = SigningKey::random(&mut csprng);
        let peer = [3u8; 32];

        let mut engine = NodeEngine::new(Blockchain::new().unwrap());
        let genesis = engine.blockchain.get_latest_block().clone();
        let block = build_empty_block(&genesis, &engine.blockchain.state, &validator, 1);

        let to_height = 1;

        let request_actions = engine.step(NodeInput::ImportRequested {
            peer,
            from_height: 1,
            to_height,
        });
        assert_eq!(engine.pending_requests.len(), 1);
        assert!(matches!(&request_actions[0], NodeAction::RequestBlocks { .. }));
        let has_more = 1 < engine.blockchain.get_latest_block().height;

        let actions = engine.step(NodeInput::FrameReceived {
            peer,
            frame: NetworkMessage::SyncResponse(SyncResponse {
                blocks: vec![block.clone()],
                has_more,
                next_height: if has_more {
                    Some(to_height.saturating_add(1))
                } else {
                    None
                },
            }).to_bytes(),
        });

        assert!(engine.pending_requests.is_empty());
        assert_eq!(engine.blockchain.head_hash, block.hash());
        assert_persist_actions(&actions, block.hash());
    }

    #[test]
    fn test_sync_request_after_sparse_restore_does_not_advertise_missing_history() {
        let mut engine = NodeEngine::new(Blockchain::new().unwrap());
        let peer = [4u8; 32];
        let restored_state = State::new();
        let restored_head = Block {
            height: 5,
            slot: 5,
            previous_hash: [7u8; 32],
            validator: rp_core::crypto::genesis_verifying_key(),
            transactions: vec![],
            signature: None,
            slash_proofs: vec![],
            state_root: restored_state.compute_state_root(),
        };

        engine.blockchain.restore_head(restored_head, restored_state);

        let actions = engine.step(NodeInput::FrameReceived {
            peer,
            frame: NetworkMessage::SyncRequest(crate::network::message::SyncRequest {
                from_height: 1,
                to_height: 5,
            }).to_bytes(),
        });

        let NodeAction::SendFrame { frame, .. } = &actions[0] else {
            panic!("expected SendFrame action");
        };
        let NetworkMessage::SyncResponse(response) =
            NetworkMessage::from_bytes(frame).unwrap() else {
            panic!("expected SyncResponse frame");
        };

        assert!(response.blocks.is_empty());
        assert!(!response.has_more);
        assert_eq!(response.next_height, Some(5));
    }

    #[test]
    fn test_empty_sync_response_with_next_height_requests_available_window() {
        let peer = [11u8; 32];
        let mut engine = NodeEngine::new(Blockchain::new().unwrap());

        let request_actions = engine.step(NodeInput::ImportRequested {
            peer,
            from_height: 1,
            to_height: 100,
        });
        assert_eq!(engine.pending_requests.len(), 1);
        assert!(matches!(&request_actions[0], NodeAction::RequestBlocks { .. }));

        let actions = engine.step(NodeInput::FrameReceived {
            peer,
            frame: NetworkMessage::SyncResponse(SyncResponse {
                blocks: vec![],
                has_more: false,
                next_height: Some(350),
            }).to_bytes(),
        });

        assert_eq!(engine.pending_requests.len(), 1);
        assert_eq!(engine.pending_requests[0].peer, peer);
        assert_eq!(engine.pending_requests[0].from_height, 350);
        assert_eq!(engine.pending_requests[0].to_height, 449);
        assert_eq!(actions.len(), 1);
        let NodeAction::RequestBlocks {
            peer: action_peer,
            from_height,
            to_height,
        } = actions[0] else {
            panic!("expected follow-up RequestBlocks action");
        };
        assert_eq!(action_peer, peer);
        assert_eq!(from_height, 350);
        assert_eq!(to_height, 449);
    }

    #[test]
    fn test_sync_response_with_more_blocks_requests_next_page() {
        let peer = [12u8; 32];
        let mut engine = NodeEngine::new(Blockchain::new().unwrap());

        let request_actions = engine.step(NodeInput::ImportRequested {
            peer,
            from_height: 1,
            to_height: 100,
        });
        assert_eq!(engine.pending_requests.len(), 1);
        assert!(matches!(&request_actions[0], NodeAction::RequestBlocks { .. }));

        let actions = engine.step(NodeInput::FrameReceived {
            peer,
            frame: NetworkMessage::SyncResponse(SyncResponse {
                blocks: vec![],
                has_more: true,
                next_height: Some(33),
            }).to_bytes(),
        });

        assert_eq!(engine.pending_requests.len(), 1);
        assert_eq!(engine.pending_requests[0].peer, peer);
        assert_eq!(engine.pending_requests[0].from_height, 33);
        assert_eq!(engine.pending_requests[0].to_height, 100);
        assert_eq!(actions.len(), 1);
        let NodeAction::RequestBlocks {
            peer: action_peer,
            from_height,
            to_height,
        } = actions[0] else {
            panic!("expected paginated RequestBlocks action");
        };
        assert_eq!(action_peer, peer);
        assert_eq!(from_height, 33);
        assert_eq!(to_height, 100);
    }

    #[test]
    fn test_peer_connected_initial_sync_records_pending_request_and_paginates() {
        let peer = [13u8; 32];
        let mut engine = NodeEngine::new(Blockchain::new().unwrap());

        let actions = engine.step(NodeInput::PeerConnected { peer });

        assert_eq!(engine.pending_requests.len(), 1);
        assert_eq!(engine.pending_requests[0].peer, peer);
        assert_eq!(engine.pending_requests[0].from_height, 1);
        assert_eq!(engine.pending_requests[0].to_height, 100);
        assert!(matches!(&actions[1], NodeAction::RequestBlocks { .. }));

        let actions = engine.step(NodeInput::FrameReceived {
            peer,
            frame: NetworkMessage::SyncResponse(SyncResponse {
                blocks: vec![],
                has_more: true,
                next_height: Some(101),
            }).to_bytes(),
        });

        assert_eq!(engine.pending_requests.len(), 1);
        assert_eq!(engine.pending_requests[0].peer, peer);
        assert_eq!(engine.pending_requests[0].from_height, 101);
        assert_eq!(engine.pending_requests[0].to_height, 200);
        assert_eq!(actions.len(), 1);

        let NodeAction::RequestBlocks {
            peer: action_peer,
            from_height,
            to_height,
        } = actions[0] else {
            panic!("expected paginated RequestBlocks action");
        };

        assert_eq!(action_peer, peer);
        assert_eq!(from_height, 101);
        assert_eq!(to_height, 200);
    }

    #[test]
    fn test_peer_transaction_relays_to_other_connected_peers() {
        let mut csprng = OsRng;
        let sender = SigningKey::random(&mut csprng);
        let receiver = SigningKey::random(&mut csprng);
        let source_peer = [8u8; 32];
        let peer_a = [9u8; 32];
        let peer_b = [10u8; 32];

        let mut engine = NodeEngine::new(Blockchain::new().unwrap());
        engine.blockchain.state.balances.insert(
            verifying_key_to_bytes(sender.verifying_key()),
            100
        );
        engine.step(NodeInput::PeerConnected { peer: source_peer });
        engine.step(NodeInput::PeerConnected { peer: peer_a });
        engine.step(NodeInput::PeerConnected { peer: peer_b });

        let transaction = build_transfer_transaction(&sender, &receiver, 25, 1, 0);
        let expected_frame = NetworkMessage::AnnounceRequest(
            AnnounceRequest::transaction(transaction.clone())
        ).to_bytes();

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
        let sender = SigningKey::random(&mut csprng);
        let receiver = SigningKey::random(&mut csprng);
        let source_peer = [11u8; 32];
        let other_peer = [12u8; 32];

        let mut engine = NodeEngine::new(Blockchain::new().unwrap());
        engine.blockchain.state.balances.insert(
            verifying_key_to_bytes(sender.verifying_key()),
            100
        );
        engine.step(NodeInput::PeerConnected { peer: source_peer });
        engine.step(NodeInput::PeerConnected { peer: other_peer });

        let transaction = build_transfer_transaction(&sender, &receiver, 25, 1, 0);
        let frame = NetworkMessage::AnnounceRequest(
            AnnounceRequest::transaction(transaction)
        ).to_bytes();

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
        let tick_actions = engine.step(NodeInput::Tick { now_ms: 1_000 });
        assert!(tick_actions.is_empty());

        let request_actions = engine.step(NodeInput::ImportRequested {
            peer,
            from_height: 1,
            to_height: 1,
        });

        assert_eq!(engine.pending_requests.len(), 1);
        assert_eq!(engine.pending_requests[0].deadline_ms, 31_000);
        assert_eq!(request_actions.len(), 2);
        assert!(matches!(request_actions[0], NodeAction::RequestBlocks { .. }));
        assert!(matches!(request_actions[1], NodeAction::ScheduleWake { at_ms: 31_000 }));

        let actions = engine.step(NodeInput::Tick { now_ms: 31_000 });

        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], NodeAction::CancelWake));
        assert!(engine.pending_requests.is_empty());
    }

    #[test]
    fn test_storage_loaded_imports_multiple_parked_siblings() {
        let mut csprng = OsRng;
        let validator_a = SigningKey::random(&mut csprng);
        let validator_b = SigningKey::random(&mut csprng);
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
            frame: NetworkMessage::AnnounceRequest(
                AnnounceRequest::block(child_a.clone())
            ).to_bytes(),
        });
        engine.step(NodeInput::FrameReceived {
            peer,
            frame: NetworkMessage::AnnounceRequest(
                AnnounceRequest::block(child_b.clone())
            ).to_bytes(),
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
