use futures::{ SinkExt, StreamExt };
use rp_node::contract::Storage;

use crate::{ storage::nvs_storage::NvsStorage };
use crate::runtime::errors::RuntimeError;
use crate::runtime::node::{ EventTx, StorageCommand, StorageRx };
pub struct StorageManager {
    storage: NvsStorage,
    event_tx: EventTx,
    storage_rx: StorageRx,
}

impl StorageManager {
    pub fn new(storage: NvsStorage, event_tx: EventTx, storage_rx: StorageRx) -> Self {
        Self { storage, event_tx, storage_rx }
    }

    pub async fn run(&mut self) -> Result<(), RuntimeError> {
        loop {
            match self.storage_rx.next().await.unwrap() {
                StorageCommand::LoadLatestSnapshot => {
                    let latest_snapshot = self.storage
                        .load_latest_snapshot_bundle()
                        .map_err(RuntimeError::from)?;

                    let (block, state_bytes) = match latest_snapshot {
                        Some((block, state_bytes)) => (Some(block), Some(state_bytes)),
                        None => (None, None),
                    };

                    self.event_tx
                        .send(crate::runtime::node::RuntimeEvent::LatestSnapshotLoaded {
                            block,
                            state_bytes,
                        }).await
                        .map_err(RuntimeError::event_send)?;
                }
                StorageCommand::LoadSnapshot { block_hash } => {
                    let state_bytes = self.storage
                        .load_snapshot(&block_hash)
                        .map_err(RuntimeError::from)?;

                    self.event_tx
                        .send(crate::runtime::node::RuntimeEvent::StorageLoaded {
                            block_hash,
                            state_bytes,
                        }).await
                        .map_err(RuntimeError::event_send)?;
                }
                StorageCommand::PersistBlock { block } => {
                    self.storage.save_block(&block).map_err(RuntimeError::from)?;
                }
                StorageCommand::PersistSnapshot { block_hash, state_bytes } => {
                    self.storage
                        .save_snapshot(&block_hash, &state_bytes)
                        .map_err(RuntimeError::from)?;
                }
            }
        }
    }
}
