use crate::mempool::Mempool;
use crate::storage::Storage;
use rp_core::models::block::{ Block, BlockNode };
use rp_core::models::transaction::Transaction;
use rp_core::state::State;
use rp_core::traits::{ Hashable };
use std::collections::HashMap;

/// The Blockchain represents the entire ledger, including the chain of blocks,
/// the current state, and the mempool of pending transactions.
#[derive(Debug)]
pub struct Blockchain {
    chain: HashMap<[u8; 32], BlockNode>,
    pub head_hash: [u8; 32],
    pub state: State,
    mempool: Mempool,
    storage: Box<dyn Storage>,
}

impl Blockchain {
    /// Creates a new blockchain with a genesis block.
    pub fn new(storage: Box<dyn Storage>) -> Self {
        let genesis_block = Block {
            height: 0,
            slot: 0,
            previous_hash: [0u8; 32],
            validator: ed25519_dalek::VerifyingKey::from_bytes(&[0u8; 32]).unwrap(),
            transactions: vec![],
            signature: None,
            slash_proofs: vec![],
            state_root: [0u8; 32],
        };
        let mut chain = HashMap::new();
        let genesis_hash = genesis_block.hash();
        chain.insert(genesis_hash, BlockNode { block: genesis_block, children: vec![] });

        let initial_state = State::new();
        storage.save_state_snapshot(&genesis_hash, &initial_state).unwrap();

        Self {
            chain,
            head_hash: genesis_hash,
            state: initial_state,
            mempool: Mempool::new(10000),
            storage,
        }
    }

    pub fn get_latest_block(&self) -> &Block {
        &self.chain
            .get(&self.head_hash)
            .expect("Blockchain should always have at least the genesis block").block
    }

    pub fn add_transaction(&mut self, tx: Transaction) -> Result<(), &'static str> {
        if self.state.is_valid_tx(&tx) {
            self.mempool.add_transaction(tx)?;
            Ok(())
        } else {
            Err("Invalid transaction")
        }
    }

    fn calculate_chain_weight(&self, tip_hash: &[u8; 32]) -> u64 {
        let mut weight = 0;
        let mut current_hash = *tip_hash;

        while let Some(node) = self.chain.get(&current_hash) {
            let validator_bytes = node.block.validator.to_bytes();
            let mut key_array = [0u8; 32];
            key_array.copy_from_slice(&validator_bytes);

            weight += self.state.stakes.get(&key_array).unwrap_or(&1);

            if node.block.height == 0 {
                break;
            }
            current_hash = node.block.previous_hash;
        }
        weight
    }

    pub fn add_block(&mut self, block: Block) -> Result<(), String> {
        if !block.is_valid() {
            return Err("Invalid block signature".to_string());
        }

        let parent_node = self.chain
            .get(&block.previous_hash)
            .ok_or_else(|| "Parent block not found (orphan)".to_string())?;
        let parent_block = &parent_node.block;

        if block.height != parent_block.height + 1 {
            return Err("Invalid block height".to_string());
        }
        if block.slot <= parent_block.slot {
            return Err("Block slot must be greater than the parent block's slot".to_string());
        }

        let mut temp_state = self.storage
            .get_state_snapshot(&block.previous_hash)?
            .ok_or_else(|| "Parent state snapshot not found".to_string())?;

        if let Some(expected_validator) = temp_state.get_expected_validator(block.height) {
            if block.validator != expected_validator {
                return Err("Invalid block validator".to_string());
            }
        }

        for tx in &block.transactions {
            if !temp_state.is_valid_tx(tx) {
                return Err("Invalid transaction in block".to_string());
            }
            temp_state.apply_tx(tx, block.slot);
        }

        for proof in &block.slash_proofs {
            if let Err(e) = temp_state.apply_slash(proof.clone()) {
                return Err(format!("Invalid slash proof: {}", e));
            }
        }

        let computed_state_root = temp_state.compute_state_root();
        if block.state_root != computed_state_root {
            return Err("Invalid state root".to_string());
        }

        let block_hash = block.hash();

        if let Err(e) = self.storage.save_block(&block) {
            return Err(format!("Failed to save block to storage: {}", e));
        }
        if let Err(e) = self.storage.save_state_snapshot(&block_hash, &temp_state) {
            return Err(format!("Failed to save state snapshot: {}", e));
        }

        if let Some(parent_node_mut) = self.chain.get_mut(&block.previous_hash) {
            parent_node_mut.children.push(block_hash);
        }
        self.chain.insert(block_hash, BlockNode { block: block.clone(), children: vec![] });

        let new_chain_weight = self.calculate_chain_weight(&block_hash);
        let current_chain_weight = self.calculate_chain_weight(&self.head_hash);

        if new_chain_weight > current_chain_weight {
            self.head_hash = block_hash;
            self.state = temp_state;

            for tx in &block.transactions {
                self.mempool.remove_transaction(&tx.hash());
            }
        }

        Ok(())
    }

    pub fn get_mempool(&self) -> Vec<Transaction> {
        self.mempool.get_pending_transactions(self.mempool.len())
    }

    pub fn get_blocks(&self, from_height: u64, to_height: u64) -> Vec<Block> {
        let mut blocks = Vec::new();
        let mut current_hash = self.head_hash;

        while let Some(node) = self.chain.get(&current_hash) {
            if node.block.height >= from_height && node.block.height <= to_height {
                blocks.push(node.block.clone());
            }
            if node.block.height == 0 {
                break;
            }
            current_hash = node.block.previous_hash;
        }
        blocks.reverse();
        blocks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{ Signer, SigningKey };
    use rand::rngs::OsRng;
    use rp_core::models::transaction::TransactionData;
    use rp_core::traits::ToBytes;
    use std::sync::Mutex;

    #[derive(Default)]
    struct TestStorage {
        blocks: Mutex<HashMap<[u8; 32], Vec<u8>>>,
        states: Mutex<HashMap<[u8; 32], State>>,
    }

    impl Storage for TestStorage {
        fn save_block(&self, block: &Block) -> Result<(), String> {
            self.blocks.lock().unwrap().insert(block.hash(), block.to_bytes());
            Ok(())
        }

        fn get_block(&self, hash: &[u8; 32]) -> Result<Option<Vec<u8>>, String> {
            Ok(self.blocks.lock().unwrap().get(hash).cloned())
        }

        fn save_state_root(&self, _height: u64, _root: &[u8; 32]) -> Result<(), String> {
            Ok(())
        }

        fn save_state_snapshot(&self, block_hash: &[u8; 32], state: &State) -> Result<(), String> {
            self.states.lock().unwrap().insert(*block_hash, state.clone());
            Ok(())
        }

        fn get_state_snapshot(&self, block_hash: &[u8; 32]) -> Result<Option<State>, String> {
            Ok(self.states.lock().unwrap().get(block_hash).cloned())
        }
    }

    #[test]
    fn test_blockchain_add_transaction_and_block() {
        let mut csprng = OsRng;
        let validator_keypair = SigningKey::generate(&mut csprng);
        let sender_keypair = SigningKey::generate(&mut csprng);
        let receiver_keypair = SigningKey::generate(&mut csprng);

        let mut blockchain = Blockchain::new(Box::new(TestStorage::default()));
        blockchain.state.balances.insert(*sender_keypair.verifying_key().as_bytes(), 100);
        blockchain.storage.save_state_snapshot(&blockchain.head_hash, &blockchain.state).unwrap();

        let mut tx = Transaction {
            sender: sender_keypair.verifying_key(),
            data: TransactionData::Transfer {
                receiver: receiver_keypair.verifying_key(),
                amount: 50,
            },
            sequence: 0,
            fee: 10,
            signature: None,
        };
        let hash = tx.hash();
        tx.signature = Some(sender_keypair.sign(&hash[..]));

        assert!(blockchain.add_transaction(tx.clone()).is_ok());
        assert_eq!(blockchain.mempool.len(), 1);

        let latest_block = blockchain.get_latest_block();
        let mut temp_state = blockchain.state.clone();
        temp_state.apply_tx(&tx, 1);
        let expected_state_root = temp_state.compute_state_root();

        let mut block = Block {
            height: latest_block.height + 1,
            slot: 1,
            previous_hash: latest_block.hash(),
            validator: validator_keypair.verifying_key(),
            transactions: vec![tx],
            signature: None,
            slash_proofs: vec![],
            state_root: expected_state_root,
        };
        let block_hash = block.hash();
        block.signature = Some(validator_keypair.sign(&block_hash[..]));

        let res = blockchain.add_block(block);
        assert!(res.is_ok(), "Failed to add block: {:?}", res.err());
        assert_eq!(blockchain.chain.len(), 2);
        assert_eq!(blockchain.mempool.len(), 0);
        assert_eq!(blockchain.state.get_balance(&sender_keypair.verifying_key()), 50);
    }

    #[test]
    fn test_enforce_consensus_validator() {
        let mut csprng = OsRng;
        let validator1 = SigningKey::generate(&mut csprng);
        let validator2 = SigningKey::generate(&mut csprng);

        let mut blockchain = Blockchain::new(Box::new(TestStorage::default()));
        blockchain.state.stakes.insert(validator1.verifying_key().to_bytes(), 100);
        blockchain.state.stakes.insert(validator2.verifying_key().to_bytes(), 200);
        blockchain.storage.save_state_snapshot(&blockchain.head_hash, &blockchain.state).unwrap();

        let expected_validator = blockchain.state.get_expected_validator(1).unwrap();

        let wrong_validator = if expected_validator == validator1.verifying_key() {
            &validator2
        } else {
            &validator1
        };

        let latest_height = blockchain.get_latest_block().height;
        let latest_hash = blockchain.get_latest_block().hash();

        let expected_state_root = blockchain.state.compute_state_root();
        let mut bad_block = Block {
            height: latest_height + 1,
            slot: 1,
            previous_hash: latest_hash,
            validator: wrong_validator.verifying_key(),
            transactions: vec![],
            signature: None,
            slash_proofs: vec![],
            state_root: expected_state_root,
        };
        let bad_hash = bad_block.hash();
        bad_block.signature = Some(wrong_validator.sign(&bad_hash[..]));

        let result = blockchain.add_block(bad_block);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Invalid block validator");

        let correct_validator = if expected_validator == validator1.verifying_key() {
            &validator1
        } else {
            &validator2
        };

        let mut good_block = Block {
            height: latest_height + 1,
            slot: 1,
            previous_hash: latest_hash,
            validator: correct_validator.verifying_key(),
            transactions: vec![],
            signature: None,
            slash_proofs: vec![],
            state_root: expected_state_root,
        };
        let good_hash = good_block.hash();
        good_block.signature = Some(correct_validator.sign(&good_hash[..]));

        assert!(blockchain.add_block(good_block).is_ok());
    }
}
