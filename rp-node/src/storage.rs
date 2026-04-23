use rp_core::{ models::block::Block, state::State };
use crate::errors::NodeError;
use core::fmt::Debug;
use alloc::vec::Vec;
/// The Storage trait defines the node-engine boundary for persisting chain data.
pub trait Storage: Send + Sync {
    fn save_block(&self, block: &Block) -> Result<(), NodeError>;
    fn get_block(&self, hash: &[u8; 32]) -> Result<Option<Vec<u8>>, NodeError>;
    fn save_state_root(&self, height: u64, root: &[u8; 32]) -> Result<(), NodeError>;
    fn save_state_snapshot(&self, block_hash: &[u8; 32], state: &State) -> Result<(), NodeError>;
    fn get_state_snapshot(&self, block_hash: &[u8; 32]) -> Result<Option<State>, NodeError>;
}

impl Debug for dyn Storage {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Storage")
    }
}
