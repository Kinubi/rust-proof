use crate::models::block::Block;
use crate::traits::ToBytes;

/// The Storage trait defines the interface for persisting blockchain data.
/// By using a trait, we can swap out the underlying database (e.g., Sled, RocksDB, or an in-memory mock for testing)
/// without changing the core Blockchain logic.
// ============================================================================
// TODO 1: Define the `Storage` trait.
// It needs methods to:
// - `save_block(&self, block: &Block) -> Result<(), String>`
// - `get_block(&self, hash: &[u8; 32]) -> Result<Option<Vec<u8>>, String>` (Returning raw bytes for now to avoid complex deserialization)
// - `save_state_root(&self, height: u64, root: &[u8; 32]) -> Result<(), String>`
// ============================================================================
pub trait Storage: Send + Sync {
    // YOUR CODE HERE
}

/// A concrete implementation of the Storage trait using the `sled` embedded database.
// ============================================================================
// TODO 2: Define the `SledStorage` struct.
// It needs to hold a `sled::Db` instance.
// ============================================================================
pub struct SledStorage {
    // YOUR CODE HERE
}

impl SledStorage {
    /// Opens a new or existing Sled database at the given path.
    // ====================================================================
    // TODO 3: Implement `SledStorage::new(path: &str)`.
    // Use `sled::open(path)` to open the database.
    // Handle the Result (e.g., unwrap or return an error).
    // ====================================================================
    pub fn new(path: &str) -> Result<Self, String> {
        unimplemented!("Implement SledStorage::new")
    }
}

// ============================================================================
// TODO 4: Implement the `Storage` trait for `SledStorage`.
// 1. `save_block`: Use `block.hash()` as the key, and `block.to_bytes()` as the value.
//    Call `self.db.insert(key, value)`.
// 2. `get_block`: Call `self.db.get(hash)`. If it returns `Some(ivec)`, convert it to a `Vec<u8>`.
// 3. `save_state_root`: Convert `height` to bytes for the key, and use `root` as the value.
// ============================================================================
// impl Storage for SledStorage { ... }

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

        // Uncomment after implementing the Storage trait
        // storage.save_block(&block).unwrap();

        // let hash = block.hash();
        // let retrieved_bytes = storage.get_block(&hash).unwrap().unwrap();

        // assert_eq!(retrieved_bytes, block.to_bytes());
    }
}
