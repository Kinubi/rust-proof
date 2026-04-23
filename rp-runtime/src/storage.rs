use rp_core::models::block::Block;
use rp_core::state::State;
use rp_core::traits::{ FromBytes, Hashable, ToBytes };
use rp_node::contract::Storage;

/// A concrete runtime storage implementation using the `sled` embedded database.
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

    fn save_state_snapshot(&self, block_hash: &[u8; 32], state: &State) -> Result<(), String> {
        let value = state.to_bytes();
        match self.db.insert(block_hash, value) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Failed to save state snapshot: {}", e)),
        }
    }

    fn get_state_snapshot(&self, block_hash: &[u8; 32]) -> Result<Option<State>, String> {
        match self.db.get(block_hash) {
            Ok(Some(ivec)) => {
                if let Ok(state) = State::from_bytes(&ivec) {
                    Ok(Some(state))
                } else {
                    Err("Failed to deserialize state snapshot".to_string())
                }
            }
            Ok(None) => Ok(None),
            Err(e) => Err(format!("Failed to get state snapshot: {}", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    #[test]
    fn test_sled_storage() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().to_str().unwrap();

        let mut csprng = OsRng;
        let validator_keypair = SigningKey::generate(&mut csprng);

        let block = Block {
            height: 1,
            slot: 1,
            previous_hash: [0; 32],
            validator: validator_keypair.verifying_key(),
            transactions: vec![],
            signature: None,
            slash_proofs: vec![],
            state_root: [0; 32],
        };

        let storage = SledStorage::new(db_path).unwrap();
        storage.save_block(&block).unwrap();

        let hash = block.hash();
        let retrieved_bytes = storage.get_block(&hash).unwrap().unwrap();

        assert_eq!(retrieved_bytes, block.to_bytes());
    }
}
