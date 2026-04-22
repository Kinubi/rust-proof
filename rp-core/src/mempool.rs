use crate::models::transaction::Transaction;
use std::collections::BTreeSet;
use crate::traits::Hashable;

/// The Mempool manages unconfirmed transactions, prioritizing them by fee.
// ============================================================================
// TODO 1: Define the `Mempool` struct.
// It needs:
// - `transactions`: A collection that keeps transactions sorted. `BTreeSet<Transaction>` is a good choice
//   if you implement `Ord` on `Transaction` to sort by fee descending.
// - `max_size`: A `usize` to limit the number of transactions.
// ============================================================================
#[derive(Debug, Clone)]
pub struct Mempool {
    transactions: BTreeSet<Transaction>,
    max_size: usize,
}

impl Mempool {
    /// Creates a new Mempool with a maximum capacity.
    pub fn new(max_size: usize) -> Self {
        // ====================================================================
        // TODO 2: Implement initialization.
        // ====================================================================
        Self {
            transactions: BTreeSet::new(),
            max_size,
        }
    }

    /// Adds a transaction to the mempool.
    /// If the mempool is full, it should reject the transaction unless its fee
    /// is higher than the lowest fee transaction currently in the pool (which should be evicted).
    pub fn add_transaction(&mut self, tx: Transaction) -> Result<(), &'static str> {
        if self.transactions.len() < self.max_size {
            self.transactions.insert(tx);
            return Ok(());
        } else {
            // Since BTreeSet is sorted descending (highest fee first),
            // the lowest fee transaction is at the end (last element).
            if let Some(lowest) = self.transactions.iter().last().cloned() {
                if tx.fee > lowest.fee {
                    self.transactions.remove(&lowest);
                    self.transactions.insert(tx);
                    return Ok(());
                } else {
                    return Err("Transaction fee too low to enter mempool");
                }
            }
            Err("Failed to evict lowest fee transaction")
        }
    }

    /// Returns up to `count` transactions with the highest fees.
    pub fn get_pending_transactions(&self, count: usize) -> Vec<Transaction> {
        self.transactions.iter().take(count).cloned().collect()
    }

    /// Removes a specific transaction by its hash.
    /// This is called when a block is added to the chain and its transactions are confirmed.
    pub fn remove_transaction(&mut self, hash: &[u8; 32]) {
        self.transactions.retain(|tx| tx.hash() != *hash);
    }

    /// Clears all transactions from the mempool.
    pub fn clear(&mut self) {
        self.transactions.clear();
    }

    /// Returns the current number of transactions in the mempool.
    pub fn len(&self) -> usize {
        self.transactions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::transaction::TransactionData;
    use ed25519_dalek::{SigningKey, Signer};
    use rand::rngs::OsRng;

    // Helper function to create a dummy transaction with a specific fee
    fn create_tx_with_fee(fee: u64, sequence: u64) -> Transaction {
        let mut csprng = OsRng;
        let sender_keypair = SigningKey::generate(&mut csprng);
        let receiver_keypair = SigningKey::generate(&mut csprng);

        let mut tx = Transaction {
            sender: sender_keypair.verifying_key(),
            data: TransactionData::Transfer {
                receiver: receiver_keypair.verifying_key(),
                amount: 10,
            },
            sequence,
            fee,
            signature: None,
        };
        
        let hash = tx.hash();
        tx.signature = Some(sender_keypair.sign(&hash[..]));
        tx
    }

    #[test]
    fn test_mempool_sorting() {
        let mut mempool = Mempool::new(10);
        
        let tx1 = create_tx_with_fee(10, 1);
        let tx2 = create_tx_with_fee(50, 2);
        let tx3 = create_tx_with_fee(5, 3);

        mempool.add_transaction(tx1.clone()).unwrap();
        mempool.add_transaction(tx2.clone()).unwrap();
        mempool.add_transaction(tx3.clone()).unwrap();

        let pending = mempool.get_pending_transactions(3);
        
        // Should be sorted descending by fee
        assert_eq!(pending.len(), 3);
        assert_eq!(pending[0].fee, 50);
        assert_eq!(pending[1].fee, 10);
        assert_eq!(pending[2].fee, 5);
    }

    #[test]
    fn test_mempool_capacity_and_eviction() {
        let mut mempool = Mempool::new(2); // Max size of 2
        
        let tx1 = create_tx_with_fee(10, 1);
        let tx2 = create_tx_with_fee(20, 2);
        
        assert!(mempool.add_transaction(tx1.clone()).is_ok());
        assert!(mempool.add_transaction(tx2.clone()).is_ok());
        assert_eq!(mempool.len(), 2);

        // Try to add a transaction with a lower fee than the lowest in the pool (10)
        let tx_low = create_tx_with_fee(5, 3);
        let result = mempool.add_transaction(tx_low);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Transaction fee too low to enter mempool");
        assert_eq!(mempool.len(), 2);

        // Try to add a transaction with a higher fee than the lowest in the pool
        let tx_high = create_tx_with_fee(30, 4);
        assert!(mempool.add_transaction(tx_high.clone()).is_ok());
        assert_eq!(mempool.len(), 2);

        // Verify the lowest fee (10) was evicted, and we kept 20 and 30
        let pending = mempool.get_pending_transactions(2);
        assert_eq!(pending[0].fee, 30);
        assert_eq!(pending[1].fee, 20);
    }

    #[test]
    fn test_remove_transaction() {
        let mut mempool = Mempool::new(10);
        let tx = create_tx_with_fee(10, 1);
        let hash = tx.hash();

        mempool.add_transaction(tx).unwrap();
        assert_eq!(mempool.len(), 1);

        mempool.remove_transaction(&hash);
        assert_eq!(mempool.len(), 0);
    }
}
