use crate::models::block::Block;
use crate::models::transaction::Transaction;
use crate::state::State;
use crate::traits::Hashable;

/// The Blockchain represents the entire ledger, including the chain of blocks,
/// the current state, and the mempool of pending transactions.
// ============================================================================
// TODO 1: Define the `Blockchain` struct.
// It needs to hold:
// - `chain`: A `Vec<Block>`.
// - `state`: A `State` instance.
// - `mempool`: A `Vec<Transaction>`.
// ============================================================================
#[derive(Debug, Clone)]
pub struct Blockchain {
    chain: Vec<Block>,
    state: State,
    mempool: Vec<Transaction>,
}

impl Blockchain {
    /// Creates a new blockchain with a genesis block.
    // ====================================================================
    // TODO 2: Implement `Blockchain::new()`.
    // 1. Create a genesis block (height 0, empty previous_hash, empty transactions).
    //    You can use a dummy validator key (e.g., all zeros).
    // 2. Initialize the `State` and `mempool`.
    // 3. Return the `Blockchain` instance with the genesis block in the chain.
    // ====================================================================
    pub fn new() -> Self {
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
            mempool: vec![],
        }
    }

    /// Returns the latest block in the chain.
    // ====================================================================
    // TODO 3: Implement `get_latest_block`.
    // Return a reference to the last block in the `chain` vector.
    // You can use `.last().expect(...)` since the chain should never be empty.
    // ====================================================================
    pub fn get_latest_block(&self) -> &Block {
        self.chain.last().expect("Blockchain should always have at least the genesis block")
    }

    /// Adds a transaction to the mempool if it is valid against the current state.
    pub fn add_transaction(&mut self, tx: Transaction) -> Result<(), &'static str> {
        // ====================================================================
        // TODO 4: Implement adding a transaction to the mempool.
        // 1. Check if the transaction is valid against the current state (`self.state.is_valid_tx(...)`).
        // 2. If valid, push it to the mempool and return `Ok(())`.
        // 3. If invalid, return `Err("Invalid transaction")`.
        // ====================================================================
        if self.state.is_valid_tx(&tx) {
            self.mempool.push(tx);
            Ok(())
        } else {
            Err("Invalid transaction")
        }
    }

    /// Adds a new block to the chain if it is valid.
    pub fn add_block(&mut self, block: Block) -> Result<(), &'static str> {
        // ====================================================================
        // TODO 5: Implement adding a block to the chain.
        // 1. Check if the block's cryptographic signature is valid (`block.is_valid()`).
        // 2. Check if the block's height is exactly one greater than the latest block's height.
        // 3. Check if the block's previous_hash matches the hash of the latest block.
        // 4. Iterate over the block's transactions and check if they are valid against the current state.
        //    (Hint: You might need to clone the state to simulate applying them, or just apply them and rollback if one fails.
        //     For simplicity, let's assume all transactions in the block must be valid against the state *as they are applied sequentially*).
        //    Actually, a simpler approach for this exercise:
        //    Create a temporary state clone. Apply each transaction to the temporary state.
        //    If any transaction is invalid, return `Err("Invalid transaction in block")`.
        //    If all are valid, replace `self.state` with the temporary state, push the block to `self.chain`, and clear the mempool (or remove the included txs).
        //    For now, just clear the mempool completely when a block is added.
        // ====================================================================
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
        let mut temp_state = self.state.clone();
        for tx in &block.transactions {
            if !temp_state.is_valid_tx(tx) {
                return Err("Invalid transaction in block");
            }
            temp_state.apply_tx(tx);
        }
        self.state = temp_state;
        self.chain.push(block);
        self.mempool.clear();
        Ok(())
    }

    pub fn get_mempool(&self) -> Vec<Transaction> {
        self.mempool.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{ SigningKey, Signer };
    use rand::rngs::OsRng;

    #[test]
    fn test_blockchain_add_transaction_and_block() {
        let mut csprng = OsRng;
        let validator_keypair = SigningKey::generate(&mut csprng);
        let sender_keypair = SigningKey::generate(&mut csprng);
        let receiver_keypair = SigningKey::generate(&mut csprng);

        let mut blockchain = Blockchain::new();
        // Give sender some initial balance
        blockchain.state.balances.insert(*sender_keypair.verifying_key().as_bytes(), 100);

        let mut tx = Transaction {
            sender: sender_keypair.verifying_key(),
            receiver: receiver_keypair.verifying_key(),
            amount: 50,
            sequence: 0,
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
}
