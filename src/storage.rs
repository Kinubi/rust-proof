use crate::models::block::Block;
use crate::traits::{ ToBytes, Hashable };
use std::fmt::Debug;
use sled;

/// The Storage trait defines the interface for persisting blockchain data.
/// By using a trait, we can swap out the underlying database (e.g., Sled, RocksDB, or an in-memory mock for testing)
/// without changing the core Blockchain logic.
pub trait Storage: Send + Sync {
    fn save_block(&self, block: &Block) -> Result<(), String>;
    fn get_block(&self, hash: &[u8; 32]) -> Result<Option<Vec<u8>>, String>;
    fn save_state_root(&self, height: u64, root: &[u8; 32]) -> Result<(), String>;
}

impl Debug for dyn Storage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Storage")
    }
}

impl Clone for Box<dyn Storage> {
    fn clone(&self) -> Box<dyn Storage> {
        // Since we can't clone a trait object directly, we can return a new instance of SledStorage.
        // In a real implementation, you would want to have a more flexible way to clone the storage.
        // For this example, we'll just create a new SledStorage with a default path.
        Box::new(SledStorage::new("default_storage_path").unwrap())
    }
}

/// A concrete implementation of the Storage trait using the `sled` embedded database.
pub struct SledStorage {
    db: sled::Db,
}

impl SledStorage {
    /// Opens a new or existing Sled database at the given path.
    pub fn new(path: &str) -> Result<Self, String> {
        match sled::open(path) {
            Ok(db) => Ok(SledStorage { db }),
            Err(e) => Err(format!("Failed to open Sled database: {}", e)),
        }
    }
}

impl Storage for SledStorage {
    fn save_block(&self, block: &Block) -> Result<(), String> {
        let key = block.hash();
        let value = block.to_bytes();
        match self.db.insert(key, value) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Failed to save block: {}", e)),
        }
    }
    fn get_block(&self, hash: &[u8; 32]) -> Result<Option<Vec<u8>>, String> {
        match self.db.get(hash) {
            Ok(Some(ivec)) => Ok(Some(ivec.to_vec())),
            Ok(None) => Ok(None),
            Err(e) => Err(format!("Failed to get block: {}", e)),
        }
    }
    fn save_state_root(&self, height: u64, root: &[u8; 32]) -> Result<(), String> {
        match self.db.insert(height.to_be_bytes(), root) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Failed to save state root: {}", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::block::Block;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    #[test]
    fn test_sled_storage() {
        // Create a temporary directory for the test database
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().to_str().unwrap();

        // Uncomment after implementing SledStorage
        // let storage = SledStorage::new(db_path).unwrap();

        let mut csprng = OsRng;
        let validator_keypair = SigningKey::generate(&mut csprng);

        let block = Block {
            height: 1,
            previous_hash: [0; 32],
            validator: validator_keypair.verifying_key(),
            transactions: vec![],
            signature: None,
        };

        let storage = SledStorage::new(db_path).unwrap();

        // Uncomment after implementing the Storage trait
        storage.save_block(&block).unwrap();

        let hash = block.hash();
        let retrieved_bytes = storage.get_block(&hash).unwrap().unwrap();

        assert_eq!(retrieved_bytes, block.to_bytes());
    }
}
