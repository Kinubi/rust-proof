use anyhow::Result;
use futures::{ SinkExt, StreamExt, TryFutureExt };
use rp_node::contract::Storage;

use crate::{ storage::nvs_storage::NvsStorage };
use crate::runtime::node::{ EventTx, StorageCommand, StorageRx };
use crate::storage::errors::StorageError;
pub struct StorageManager {
    storage: NvsStorage,
    event_tx: EventTx,
    storage_rx: StorageRx,
}

impl StorageManager {
    pub fn new(storage: NvsStorage, event_tx: EventTx, storage_rx: StorageRx) -> Self {
        Self { storage, event_tx, storage_rx }
    }

    pub async fn run(&mut self) -> Result<(), StorageError> {
        loop {
            match self.storage_rx.next().await.unwrap() {
                StorageCommand::LoadSnapshot { block_hash } => {
                    if
                        let Ok(state_bytes) = self.storage
                            .load_snapshot(&block_hash)
                            .map_err(StorageError::ContractError)
                    {
                        let _ = self.event_tx
                            .send(crate::runtime::node::RuntimeEvent::StorageLoaded {
                                block_hash,
                                state_bytes,
                            })
                            .map_err(StorageError::StorageChannelSendError);
                    }
                }
                StorageCommand::PersistBlock { block } => {
                    let _ = self.storage.save_block(&block).map_err(StorageError::ContractError);
                }
                StorageCommand::PersistSnapshot { block_hash, state_bytes } => {
                    let _ = self.storage
                        .save_snapshot(&block_hash, &state_bytes)
                        .map_err(StorageError::ContractError);
                }
            }
        }
    }
}
