use crate::models::block::Block;
use crate::models::transaction::Transaction;
use serde::{ Serialize, Deserialize };

// ============================================================================
// TODO: Chapter 8 - Define Network Messages
// 1. Define an enum `NetworkMessage` that implements `Serialize` and `Deserialize`.
// 2. Add variants for:
//    - `NewTransaction(Transaction)`
//    - `NewBlock(Block)`
//    - `SyncRequest { from_height: u64, to_height: u64 }`
//    - `SyncResponse { blocks: Vec<Block> }`
// ============================================================================
#[derive(Serialize, Deserialize, Debug)]
pub enum NetworkMessage {
    NewTransaction(Transaction),
    NewBlock(Block),
    SyncRequest {
        from_height: u64,
        to_height: u64,
    },
    SyncResponse {
        blocks: Vec<Block>,
    },
}
