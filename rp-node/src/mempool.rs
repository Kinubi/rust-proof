use rp_core::models::transaction::Transaction;
use rp_core::traits::Hashable;
use alloc::collections::BTreeSet;
use alloc::vec::Vec;

/// The Mempool manages unconfirmed transactions, prioritizing them by fee.
#[derive(Debug, Clone)]
pub struct Mempool {
    transactions: BTreeSet<Transaction>,
    max_size: usize,
}

impl Mempool {
    pub fn new(max_size: usize) -> Self {
        Self {
            transactions: BTreeSet::new(),
            max_size,
        }
    }

    pub fn add_transaction(&mut self, tx: Transaction) -> Result<bool, &'static str> {
        if self.transactions.contains(&tx) {
            return Ok(false);
        }

        if self.transactions.len() < self.max_size {
            self.transactions.insert(tx);
            return Ok(true);
        } else {
            if let Some(lowest) = self.transactions.iter().last().cloned() {
                if tx.fee > lowest.fee {
                    self.transactions.remove(&lowest);
                    self.transactions.insert(tx);
                    return Ok(true);
                } else {
                    return Err("Transaction fee too low to enter mempool");
                }
            }
            Err("Failed to evict lowest fee transaction")
        }
    }

    pub fn get_pending_transactions(&self, count: usize) -> Vec<Transaction> {
        self.transactions.iter().take(count).cloned().collect()
    }

    pub fn remove_transaction(&mut self, hash: &[u8; 32]) {
        self.transactions.retain(|tx| tx.hash() != *hash);
    }

    pub fn clear(&mut self) {
        self.transactions.clear();
    }

    pub fn len(&self) -> usize {
        self.transactions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{ Signer, SigningKey };
    use rand::rngs::OsRng;
    use rp_core::models::transaction::TransactionData;

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
        assert_eq!(pending.len(), 3);
        assert_eq!(pending[0].fee, 50);
        assert_eq!(pending[1].fee, 10);
        assert_eq!(pending[2].fee, 5);
    }

    #[test]
    fn test_mempool_capacity_and_eviction() {
        let mut mempool = Mempool::new(2);
        let tx1 = create_tx_with_fee(10, 1);
        let tx2 = create_tx_with_fee(20, 2);

        assert!(mempool.add_transaction(tx1.clone()).is_ok());
        assert!(mempool.add_transaction(tx2.clone()).is_ok());
        assert_eq!(mempool.len(), 2);

        let tx_low = create_tx_with_fee(5, 3);
        let result = mempool.add_transaction(tx_low);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Transaction fee too low to enter mempool");
        assert_eq!(mempool.len(), 2);

        let tx_high = create_tx_with_fee(30, 4);
        assert!(mempool.add_transaction(tx_high.clone()).is_ok());
        assert_eq!(mempool.len(), 2);

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

    #[test]
    fn test_duplicate_transaction_returns_false() {
        let mut mempool = Mempool::new(10);
        let tx = create_tx_with_fee(10, 1);

        assert_eq!(mempool.add_transaction(tx.clone()), Ok(true));
        assert_eq!(mempool.add_transaction(tx), Ok(false));
        assert_eq!(mempool.len(), 1);
    }
}
