use rp_core::models::block::Block;
use rp_core::models::transaction::Transaction;
use serde::{ Deserialize, Serialize };

#[derive(Serialize, Deserialize, Debug)]
pub enum NetworkMessage {
    NewTransaction(Transaction),
    NewBlock(Block),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SyncRequest {
    pub from_height: u64,
    pub to_height: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SyncResponse {
    pub blocks: Vec<Block>,
}
