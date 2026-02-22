use crate::models::block::{ Block, BlockNode };
use crate::models::transaction::Transaction;
use crate::state::State;
use crate::storage::Storage;
use crate::traits::Hashable;
use crate::mempool::Mempool;
use std::collections::HashMap;
/// The Blockchain represents the entire ledger, including the chain of blocks,
/// the current state, and the mempool of pending transactions.
#[derive(Debug, Clone)]
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
            mempool: Mempool::new(10000), // Set a default max size for the mempool
            storage,
        }
    }

    /// Returns the latest block in the chain.
    pub fn get_latest_block(&self) -> &Block {
        &self.chain
            .get(&self.head_hash)
            .expect("Blockchain should always have at least the genesis block").block
    }

    /// Adds a transaction to the mempool if it is valid against the current state.
    pub fn add_transaction(&mut self, tx: Transaction) -> Result<(), &'static str> {
        if self.state.is_valid_tx(&tx) {
            self.mempool.add_transaction(tx)?;
            Ok(())
        } else {
            Err("Invalid transaction")
        }
    }

    /// Calculates the total weight of a chain ending at `tip_hash`.
    /// For this simple PoS, weight is the sum of the stakes of the validators who proposed the blocks.
    fn calculate_chain_weight(&self, tip_hash: &[u8; 32]) -> u64 {
        let mut weight = 0;
        let mut current_hash = *tip_hash;

        while let Some(node) = self.chain.get(&current_hash) {
            let validator_bytes = node.block.validator.to_bytes();
            let mut key_array = [0u8; 32];
            key_array.copy_from_slice(&validator_bytes);

            // Use the current state's stakes as an approximation for the weight.
            // In a real system, you'd use the stake at the time the block was proposed.
            weight += self.state.stakes.get(&key_array).unwrap_or(&1);

            if node.block.height == 0 {
                break;
            }
            current_hash = node.block.previous_hash;
        }
        weight
    }

    /// Adds a new block to the chain if it is valid.
    pub fn add_block(&mut self, block: Block) -> Result<(), String> {
        if !block.is_valid() {
            return Err("Invalid block signature".to_string());
        }

        // 1. Find Parent
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

        // 2. Load Parent State
        let mut temp_state = self.storage
            .get_state_snapshot(&block.previous_hash)?
            .ok_or_else(|| "Parent state snapshot not found".to_string())?;

        // Verify validator is expected (using the parent's state)
        if let Some(expected_validator) = temp_state.get_expected_validator(block.height) {
            if block.validator != expected_validator {
                return Err("Invalid block validator".to_string());
            }
        }

        // 3. Apply & Verify Transactions
        for tx in &block.transactions {
            if !temp_state.is_valid_tx(tx) {
                return Err("Invalid transaction in block".to_string());
            }
            temp_state.apply_tx(tx, block.slot);
        }

        // Apply slash proofs
        for proof in &block.slash_proofs {
            if let Err(e) = temp_state.apply_slash(proof.clone()) {
                return Err(format!("Invalid slash proof: {}", e));
            }
        }

        // 4. State Root Check
        let computed_state_root = temp_state.compute_state_root();
        if block.state_root != computed_state_root {
            // For testing purposes, we might want to just log this or update it if it's a dummy value.
            // But strictly, it should be rejected.
            return Err("Invalid state root".to_string());
        }

        // 5. Save
        let block_hash = block.hash();

        if let Err(e) = self.storage.save_block(&block) {
            return Err(format!("Failed to save block to storage: {}", e));
        }
        if let Err(e) = self.storage.save_state_snapshot(&block_hash, &temp_state) {
            return Err(format!("Failed to save state snapshot: {}", e));
        }

        // Update parent's children list
        if let Some(parent_node_mut) = self.chain.get_mut(&block.previous_hash) {
            parent_node_mut.children.push(block_hash);
        }
        self.chain.insert(block_hash, BlockNode { block: block.clone(), children: vec![] });

        // 6. Fork Choice (Heaviest Chain)
        let new_chain_weight = self.calculate_chain_weight(&block_hash);
        let current_chain_weight = self.calculate_chain_weight(&self.head_hash);

        if new_chain_weight > current_chain_weight {
            // State Reorg!
            self.head_hash = block_hash;
            self.state = temp_state;

            // Clean up mempool based on the new canonical chain
            for tx in &block.transactions {
                self.mempool.remove_transaction(&tx.hash());
            }
        }

        Ok(())
    }

    pub fn get_mempool(&self) -> Vec<Transaction> {
        self.mempool.get_pending_transactions(self.mempool.len())
    }
}

#[cfg(test)]
mod tests {
    use crate::{ models::transaction::TransactionData, storage::SledStorage };

    use super::*;
    use ed25519_dalek::{ SigningKey, Signer };
    use rand::rngs::OsRng;

    #[test]
    fn test_blockchain_add_transaction_and_block() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut csprng = OsRng;
        let validator_keypair = SigningKey::generate(&mut csprng);
        let sender_keypair = SigningKey::generate(&mut csprng);
        let receiver_keypair = SigningKey::generate(&mut csprng);

        let mut blockchain = Blockchain::new(
            Box::new(SledStorage::new(temp_dir.path().to_str().unwrap()).unwrap())
        );
        // Give sender some initial balance
        blockchain.state.balances.insert(*sender_keypair.verifying_key().as_bytes(), 100);
        // Update the genesis state snapshot in storage so add_block sees the balance
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

        // Add transaction to mempool
        assert!(blockchain.add_transaction(tx.clone()).is_ok());
        assert_eq!(blockchain.mempool.len(), 1);

        // Create a block
        let latest_block = blockchain.get_latest_block();

        // Compute the expected state root
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

        // Add block to chain
        let res = blockchain.add_block(block);
        assert!(res.is_ok(), "Failed to add block: {:?}", res.err());
        assert_eq!(blockchain.chain.len(), 2);
        assert_eq!(blockchain.mempool.len(), 0);
        assert_eq!(blockchain.state.get_balance(&sender_keypair.verifying_key()), 50);
    }

    #[test]
    fn test_enforce_consensus_validator() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut csprng = OsRng;
        let validator1 = SigningKey::generate(&mut csprng);
        let validator2 = SigningKey::generate(&mut csprng);

        let mut blockchain = Blockchain::new(
            Box::new(SledStorage::new(temp_dir.path().to_str().unwrap()).unwrap())
        );

        // Add stakes to the state so we have a validator pool
        blockchain.state.stakes.insert(validator1.verifying_key().to_bytes(), 100);
        blockchain.state.stakes.insert(validator2.verifying_key().to_bytes(), 200);
        // Update the genesis state snapshot in storage so add_block sees the stakes
        blockchain.storage.save_state_snapshot(&blockchain.head_hash, &blockchain.state).unwrap();

        // Find out who is SUPPOSED to forge block 1
        let expected_validator = blockchain.state.get_expected_validator(1).unwrap();

        // Figure out who the "wrong" validator is
        let wrong_validator = if expected_validator == validator1.verifying_key() {
            &validator2
        } else {
            &validator1
        };

        // We need to extract the height and hash so we don't hold an immutable borrow
        // on `blockchain` while trying to call `blockchain.add_block` (which requires a mutable borrow).
        let latest_height = blockchain.get_latest_block().height;
        let latest_hash = blockchain.get_latest_block().hash();

        // 1. Try to add a block forged by the WRONG validator
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
        assert!(
            result.is_err(),
            "Block should be rejected because it was forged by the wrong validator"
        );
        assert_eq!(result.unwrap_err(), "Invalid block validator");

        // 2. Try to add a block forged by the CORRECT validator
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

        assert!(
            blockchain.add_block(good_block).is_ok(),
            "Block should be accepted because it was forged by the correct validator"
        );
    }
}
