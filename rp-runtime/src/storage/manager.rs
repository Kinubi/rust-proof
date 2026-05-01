use std::path::Path;

use log::warn;
use rp_core::{ models::block::Block, traits::Hashable };
use rp_node::{ contract::{ BlockHash, Storage }, errors::ContractError };

use crate::runtime::{
    errors::RuntimeError,
    manager::{ EventTx, RuntimeEvent, StorageCommand, StorageRx },
};

const META_TREE: &str = "meta";
const BLOCKS_TREE: &str = "blocks";
const SNAPSHOTS_TREE: &str = "snapshots";
const LATEST_SNAPSHOT_KEY: &[u8] = b"latest_snapshot";
const TAG: &str = "storage";

pub struct SledStorage {
    db: sled::Db,
    meta: sled::Tree,
    blocks: sled::Tree,
    snapshots: sled::Tree,
}

impl SledStorage {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, sled::Error> {
        let db = sled::open(path)?;
        let meta = db.open_tree(META_TREE)?;
        let blocks = db.open_tree(BLOCKS_TREE)?;
        let snapshots = db.open_tree(SNAPSHOTS_TREE)?;

        Ok(Self {
            db,
            meta,
            blocks,
            snapshots,
        })
    }

    fn map_storage_error<T>(_error: T) -> ContractError {
        ContractError::Storage
    }

    pub fn load_latest_snapshot_bundle(
        &mut self
    ) -> Result<Option<(Block, Vec<u8>)>, ContractError> {
        let block_hash = match self.meta.get(LATEST_SNAPSHOT_KEY).map_err(Self::map_storage_error)? {
            Some(bytes) if bytes.len() == 32 => {
                let mut hash = [0u8; 32];
                hash.copy_from_slice(bytes.as_ref());
                hash
            }
            Some(_) => {
                warn!(target: TAG, "latest snapshot pointer has invalid hash bytes; starting without a restored snapshot");
                return Ok(None);
            }
            None => {
                return Ok(None);
            }
        };

        let block = match self.load_block(&block_hash)? {
            Some(block) => block,
            None => {
                warn!(target: TAG, "latest snapshot pointer references a missing block; starting without a restored snapshot");
                return Ok(None);
            }
        };

        let state_bytes = match self.load_snapshot(&block_hash)? {
            Some(state_bytes) => state_bytes,
            None => {
                warn!(target: TAG, "latest snapshot pointer references missing snapshot bytes; starting without a restored snapshot");
                return Ok(None);
            }
        };

        Ok(Some((block, state_bytes)))
    }

    fn flush(&self) -> Result<(), ContractError> {
        self.db.flush().map_err(Self::map_storage_error)?;
        Ok(())
    }
}

impl Storage for SledStorage {
    fn save_block(&mut self, block: &Block) -> Result<(), ContractError> {
        let block_hash = block.hash();
        let block_bytes = postcard::to_allocvec(block).map_err(|_| ContractError::Storage)?;
        self.blocks.insert(block_hash, block_bytes).map_err(Self::map_storage_error)?;
        self.flush()
    }

    fn load_block(&mut self, hash: &BlockHash) -> Result<Option<Block>, ContractError> {
        let Some(block_bytes) = self.blocks.get(hash).map_err(Self::map_storage_error)? else {
            return Ok(None);
        };

        let block: Block = postcard::from_bytes(&block_bytes).map_err(|_| ContractError::Storage)?;
        if block.hash() != *hash {
            return Err(ContractError::Storage);
        }

        Ok(Some(block))
    }

    fn save_snapshot(
        &mut self,
        block_hash: &BlockHash,
        state_bytes: &[u8]
    ) -> Result<(), ContractError> {
        self.snapshots.insert(block_hash, state_bytes).map_err(Self::map_storage_error)?;
        self.meta.insert(LATEST_SNAPSHOT_KEY, block_hash).map_err(Self::map_storage_error)?;
        self.flush()
    }

    fn load_snapshot(&mut self, block_hash: &BlockHash) -> Result<Option<Vec<u8>>, ContractError> {
        Ok(
            self.snapshots
                .get(block_hash)
                .map_err(Self::map_storage_error)?
                .map(|bytes| bytes.to_vec())
        )
    }
}

pub struct StorageManager {
    storage: SledStorage,
    event_tx: EventTx,
    storage_rx: StorageRx,
}

impl StorageManager {
    pub fn new(storage: SledStorage, event_tx: EventTx, storage_rx: StorageRx) -> Self {
        Self {
            storage,
            event_tx,
            storage_rx,
        }
    }

    pub async fn run(&mut self) -> Result<(), RuntimeError> {
        loop {
            let Some(command) = self.storage_rx.recv().await else {
                return Ok(());
            };

            match command {
                StorageCommand::LoadLatestSnapshot => {
                    let latest_snapshot = self.storage
                        .load_latest_snapshot_bundle()
                        .map_err(RuntimeError::from)?;

                    let (block, state_bytes) = match latest_snapshot {
                        Some((block, state_bytes)) => (Some(block), Some(state_bytes)),
                        None => (None, None),
                    };

                    self.event_tx
                        .send(RuntimeEvent::LatestSnapshotLoaded { block, state_bytes }).await
                        .map_err(RuntimeError::event_send)?;
                }
                StorageCommand::LoadSnapshot { block_hash } => {
                    let state_bytes = self.storage
                        .load_snapshot(&block_hash)
                        .map_err(RuntimeError::from)?;

                    self.event_tx
                        .send(RuntimeEvent::StorageLoaded {
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

#[cfg(test)]
mod tests {
    use super::*;

    use rand::rngs::OsRng;
    use rp_core::crypto::{ Signer, SigningKey, genesis_verifying_key };
    use tempfile::tempdir;

    fn build_block() -> Block {
        let mut csprng = OsRng;
        let validator = SigningKey::random(&mut csprng);
        let mut block = Block {
            height: 1,
            slot: 1,
            previous_hash: [0u8; 32],
            validator: genesis_verifying_key(),
            transactions: vec![],
            signature: None,
            slash_proofs: vec![],
            state_root: [1u8; 32],
        };
        let hash = block.hash();
        block.validator = validator.verifying_key().clone();
        block.signature = Some(validator.sign(&hash));
        block
    }

    #[test]
    fn load_latest_snapshot_bundle_round_trips_saved_block_and_snapshot() {
        let temp_dir = tempdir().unwrap();
        let mut storage = SledStorage::new(temp_dir.path()).unwrap();
        let block = build_block();
        let state_bytes = vec![1u8, 2, 3, 4];
        let block_hash = block.hash();

        storage.save_block(&block).unwrap();
        storage.save_snapshot(&block_hash, &state_bytes).unwrap();

        let (loaded_block, loaded_state_bytes) = storage
            .load_latest_snapshot_bundle()
            .unwrap()
            .unwrap();

        assert_eq!(loaded_block.hash(), block_hash);
        assert_eq!(loaded_state_bytes, state_bytes);
    }
}
