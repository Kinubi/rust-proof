use rp_core::{ models::block::Block, state::State };
use std::fmt::Debug;

/// The Storage trait defines the node-engine boundary for persisting chain data.
pub trait Storage: Send + Sync {
    fn save_block(&self, block: &Block) -> Result<(), String>;
    fn get_block(&self, hash: &[u8; 32]) -> Result<Option<Vec<u8>>, String>;
    fn save_state_root(&self, height: u64, root: &[u8; 32]) -> Result<(), String>;
    fn save_state_snapshot(&self, block_hash: &[u8; 32], state: &State) -> Result<(), String>;
    fn get_state_snapshot(&self, block_hash: &[u8; 32]) -> Result<Option<State>, String>;
}

impl Debug for dyn Storage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Storage")
    }
}
