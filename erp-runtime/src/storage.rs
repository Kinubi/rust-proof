// use rp_node::contract::Storage;

// pub struct PSRAMStorage {
//     in_memory_db: [u8; 128],
// }

// impl PSRAMStorage {
//     pub fn new() {}
// }

// impl Storage for PSRAMStorage {
//     fn save_block(&mut self, block: &Block) -> Result<(), ContractError>;
//     fn load_block(&mut self, hash: &BlockHash) -> Result<Option<Block>, ContractError>;

//     fn save_snapshot(
//         &mut self,
//         block_hash: &BlockHash,
//         state_bytes: &[u8]
//     ) -> Result<(), ContractError>;
//     fn load_snapshot(&mut self, block_hash: &BlockHash) -> Result<Option<Vec<u8>>, ContractError>;
// }
