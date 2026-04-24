use futures::{ SinkExt, StreamExt };
use rp_core::models::block::Block;
use rp_node::node_engine::{ NodeEngine };
use rp_node::contract::{ BlockHash, NodeAction, NodeInput, PeerId };
use futures::channel::mpsc;
use log::info;

use crate::runtime::errors::RuntimeError;

const TAG: &str = "node";

pub enum RuntimeEvent {
    Tick {
        now_ms: u64,
    },
    PeerConnected {
        peer: PeerId,
    },
    PeerDisconnected {
        peer: PeerId,
    },
    FrameReceived {
        peer: PeerId,
        frame: Vec<u8>,
    },
    StorageLoaded {
        block_hash: [u8; 32],
        state_bytes: Option<Vec<u8>>,
    },
    LatestSnapshotLoaded {
        block: Option<Block>,
        state_bytes: Option<Vec<u8>>,
    },
}

pub enum NetworkCommand {
    SendFrame {
        peer: PeerId,
        frame: Vec<u8>,
    },
    BroadcastFrame {
        frame: Vec<u8>,
    },
    DisconnectPeer {
        peer: PeerId,
    },
    RequestBlocks {
        peer: PeerId,
        from_height: u64,
        to_height: u64,
    },
}

pub enum StorageCommand {
    LoadLatestSnapshot,
    PersistBlock {
        block: Block,
    },
    PersistSnapshot {
        block_hash: BlockHash,
        state_bytes: Vec<u8>,
    },
    LoadSnapshot {
        block_hash: BlockHash,
    },
}

pub enum WakeCommand {
    Schedule {
        at_ms: u64,
    },
    Cancel,
}

pub type EventTx = mpsc::Sender<RuntimeEvent>;
pub type EventRx = mpsc::Receiver<RuntimeEvent>;
pub type NetworkTx = mpsc::Sender<NetworkCommand>;
pub type NetworkRx = mpsc::Receiver<NetworkCommand>;
pub type StorageTx = mpsc::Sender<StorageCommand>;
pub type StorageRx = mpsc::Receiver<StorageCommand>;
pub type WakeTx = mpsc::Sender<WakeCommand>;
pub type WakeRx = mpsc::Receiver<WakeCommand>;

pub struct NodeManager {
    node_engine: NodeEngine,
    event_rx: EventRx,
    network_tx: NetworkTx,
    storage_tx: StorageTx,
    wake_tx: WakeTx,
}
impl NodeManager {
    pub fn new(
        node_engine: NodeEngine,
        event_rx: EventRx,
        network_tx: NetworkTx,
        storage_tx: StorageTx,
        wake_tx: WakeTx
    ) -> Self {
        Self { node_engine, event_rx, network_tx, storage_tx, wake_tx }
    }

    pub async fn run(&mut self) -> Result<(), RuntimeError> {
        info!(target: TAG, "Running Runtime");
        info!(target: TAG, "Loading latest snapshot");
        self.storage_tx
            .send(StorageCommand::LoadLatestSnapshot).await
            .map_err(RuntimeError::storage_send)?;

        loop {
            let actions = match self.event_rx.next().await.unwrap() {
                RuntimeEvent::LatestSnapshotLoaded { block, state_bytes } => {
                    self.node_engine.restore_latest_snapshot(block, state_bytes)
                }
                RuntimeEvent::Tick { now_ms } => {
                    self.node_engine.step(NodeInput::Tick { now_ms })
                }
                RuntimeEvent::PeerConnected { peer } => {
                    self.node_engine.step(NodeInput::PeerConnected { peer })
                }
                RuntimeEvent::PeerDisconnected { peer } => {
                    self.node_engine.step(NodeInput::PeerDisconnected { peer })
                }
                RuntimeEvent::FrameReceived { peer, frame } => {
                    info!(target: TAG, "Frame: {:?} received", frame);
                    self.node_engine.step(NodeInput::FrameReceived { peer, frame })
                }
                RuntimeEvent::StorageLoaded { block_hash, state_bytes } => {
                    self.node_engine.step(NodeInput::StorageLoaded {
                        block_hash,
                        state_bytes,
                    })
                }
            };

            for action in actions {
                self.handle_action(action).await?;
            }
        }
    }

    async fn handle_action(&mut self, action: NodeAction) -> Result<(), RuntimeError> {
        match action {
            NodeAction::BroadcastFrame { frame } => {
                self.network_tx
                    .send(NetworkCommand::BroadcastFrame { frame }).await
                    .map_err(RuntimeError::network_send)
            }
            NodeAction::DisconnectPeer { peer } => {
                self.network_tx
                    .send(NetworkCommand::DisconnectPeer { peer }).await
                    .map_err(RuntimeError::network_send)
            }
            NodeAction::FrameReceived { peer } => {
                info!(target: TAG, "Peer frame from {:?} received, but no action", peer);
                Ok(())
            }
            NodeAction::LoadSnapshot { block_hash } => {
                self.storage_tx
                    .send(StorageCommand::LoadSnapshot { block_hash }).await
                    .map_err(RuntimeError::storage_send)
            }
            NodeAction::PersistBlock { block } => {
                self.storage_tx
                    .send(StorageCommand::PersistBlock { block }).await
                    .map_err(RuntimeError::storage_send)
            }
            NodeAction::PersistCompleted { persist_type } => {
                info!(target: TAG, "Persist: {:?} completed", persist_type);
                Ok(())
            }
            NodeAction::PersistSnapshot { block_hash, state_bytes } => {
                self.storage_tx
                    .send(StorageCommand::PersistSnapshot {
                        block_hash,
                        state_bytes,
                    }).await
                    .map_err(RuntimeError::storage_send)
            }
            NodeAction::ReportEvent { message } => {
                info!(target: TAG, "{message}");
                Ok(())
            }
            NodeAction::RequestBlocks { peer, from_height, to_height } => {
                self.network_tx
                    .send(NetworkCommand::RequestBlocks {
                        peer,
                        from_height,
                        to_height,
                    }).await
                    .map_err(RuntimeError::network_send)
            }
            NodeAction::ScheduleWake { at_ms } => {
                self.wake_tx
                    .send(WakeCommand::Schedule { at_ms }).await
                    .map_err(RuntimeError::wake_send)
            }
            NodeAction::SendFrame { peer, frame } => {
                self.network_tx
                    .send(NetworkCommand::SendFrame { peer, frame }).await
                    .map_err(RuntimeError::network_send)
            }
        }
    }
}
