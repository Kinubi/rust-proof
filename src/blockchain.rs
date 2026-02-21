use crate::models::block::Block;
use crate::models::transaction::Transaction;
use crate::state::State;
use crate::storage::Storage;
use crate::traits::Hashable;
use crate::mempool::Mempool;

/// The Blockchain represents the entire ledger, including the chain of blocks,
/// the current state, and the mempool of pending transactions.
#[derive(Debug, Clone)]
pub struct Blockchain {
    chain: Vec<Block>,
    state: State,
    mempool: Mempool,
    storage: Box<dyn Storage>,
}

impl Blockchain {
    /// Creates a new blockchain with a genesis block.
    pub fn new(storage: Box<dyn Storage>) -> Self {
        let genesis_block = Block {
            height: 0,
            previous_hash: [0u8; 32],
            validator: ed25519_dalek::VerifyingKey::from_bytes(&[0u8; 32]).unwrap(),
            transactions: vec![],
            signature: None,
        };
        Self {
            chain: vec![genesis_block],
            state: State::new(),
            mempool: Mempool::new(10000), // Set a default max size for the mempool
            storage,
        }
    }

    /// Returns the latest block in the chain.
    pub fn get_latest_block(&self) -> &Block {
        self.chain.last().expect("Blockchain should always have at least the genesis block")
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

    /// Adds a new block to the chain if it is valid.
    pub fn add_block(&mut self, block: Block) -> Result<(), &'static str> {
        if !block.is_valid() {
            return Err("Invalid block signature");
        }
        let latest_block = self.get_latest_block();
        if block.height != latest_block.height + 1 {
            return Err("Invalid block height");
        }
        if block.previous_hash != latest_block.hash() {
            return Err("Invalid previous hash");
        }
        if let Some(expected_validator) = self.state.get_expected_validator(block.height) {
            if block.validator != expected_validator {
                return Err("Invalid block validator");
            }
        }
        let mut temp_state = self.state.clone();
        for tx in &block.transactions {
            if !temp_state.is_valid_tx(tx) {
                return Err("Invalid transaction in block");
            }
            temp_state.apply_tx(tx);
        }
        self.state = temp_state;
        if let Err(e) = self.storage.save_block(&block) {
            return Err("Failed to save block to storage");
        }
        if
            let Err(e) = self.storage.save_state_root(
                block.height,
                &self.state.compute_state_root()
            )
        {
            return Err("Failed to save state root to storage");
        }
        self.chain.push(block);
        for tx in &self.chain.last().unwrap().transactions {
            self.mempool.remove_transaction(&tx.hash());
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
        let mut block = Block {
            height: latest_block.height + 1,
            previous_hash: latest_block.hash(),
            validator: validator_keypair.verifying_key(),
            transactions: vec![tx],
            signature: None,
        };
        let block_hash = block.hash();
        block.signature = Some(validator_keypair.sign(&block_hash[..]));

        // Add block to chain
        assert!(blockchain.add_block(block).is_ok());
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
        let mut bad_block = Block {
            height: latest_height + 1,
            previous_hash: latest_hash,
            validator: wrong_validator.verifying_key(),
            transactions: vec![],
            signature: None,
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
            previous_hash: latest_hash,
            validator: correct_validator.verifying_key(),
            transactions: vec![],
            signature: None,
        };
        let good_hash = good_block.hash();
        good_block.signature = Some(correct_validator.sign(&good_hash[..]));

        assert!(
            blockchain.add_block(good_block).is_ok(),
            "Block should be accepted because it was forged by the correct validator"
        );
    }
}
