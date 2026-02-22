use crate::models::block::Block;
use crate::models::transaction::Transaction;

// ============================================================================
// TODO: Chapter 8 - Define Network Messages
// 1. Add serde derives (Serialize, Deserialize) once added to Cargo.toml
// 2. Define an enum `NetworkMessage` with variants for:
//    - Handshake { height: u64, genesis_hash: [u8; 32] }
//    - Transaction(Transaction)
//    - Block(Block)
//    - GetBlocks { from_height: u64, to_height: u64 }
// ============================================================================
