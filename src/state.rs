use std::collections::HashMap;
use ed25519_dalek::VerifyingKey;
use crate::models::transaction::{ Transaction, TransactionData };
use crate::traits::{ ToBytes, Hashable };

/// The State represents the current balances and sequence numbers of all accounts.
// ============================================================================
// TODO 1: Define the `State` struct.
// It needs to keep track of:
// - `balances`: A mapping from an account's public key (as `[u8; 32]`) to its balance (`u64`).
// - `nonces`: A mapping from an account's public key to its next expected sequence number (`u64`).
// Hint: Use `std::collections::HashMap`.
// ============================================================================
#[derive(Debug, Clone, Default)]
pub struct State {
    pub balances: HashMap<[u8; 32], u64>,
    pub nonces: HashMap<[u8; 32], u64>,
    pub stakes: HashMap<[u8; 32], u64>,
}

impl State {
    // ====================================================================
    // TODO 2: Implement `State::new()`.
    // Initialize the HashMaps.
    // ====================================================================
    pub fn new() -> Self {
        Self {
            balances: HashMap::new(),
            nonces: HashMap::new(),
            stakes: HashMap::new(),
        }
    }

    /// Returns the balance of an account.
    // ====================================================================
    // TODO 3: Implement `get_balance`.
    // Return the balance if it exists, otherwise return 0.
    // ====================================================================
    pub fn get_balance(&self, account: &VerifyingKey) -> u64 {
        let key_bytes = account.to_bytes();
        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(&key_bytes);
        *self.balances.get(&key_array).unwrap_or(&0)
    }

    /// Returns the next expected sequence number for an account.
    // ====================================================================
    // TODO 4: Implement `get_nonce`.
    // Return the nonce if it exists, otherwise return 0.
    // ====================================================================
    pub fn get_nonce(&self, account: &VerifyingKey) -> u64 {
        let key_bytes = account.to_bytes();
        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(&key_bytes);
        *self.nonces.get(&key_array).unwrap_or(&0)
    }

    /// Validates a transaction against the current state.
    /// Returns true if the transaction is valid, false otherwise.
    pub fn is_valid_tx(&self, tx: &Transaction) -> bool {
        // ====================================================================
        // TODO 5: Implement state validation for a transaction.
        // 1. Check if the transaction's cryptographic signature is valid (use `tx.is_valid()`).
        // 2. Check if the sender has enough balance (`self.get_balance(...) >= tx.amount`).
        // 3. Check if the transaction's sequence number matches the expected nonce (`self.get_nonce(...) == tx.sequence`).
        // ====================================================================
        let tx_valid = tx.is_valid();
        let sender_balance = self.get_balance(&tx.sender);
        match &tx.data {
            TransactionData::Transfer { amount, .. } => {
                if sender_balance < *amount {
                    return false;
                }
            }
            TransactionData::Stake { amount } => {
                if sender_balance < *amount {
                    return false;
                }
            }
        }
        let sender_nonce = self.get_nonce(&tx.sender);
        if tx.sequence != sender_nonce {
            return false;
        }
        tx_valid
    }

    /// Applies a transaction to the state, updating balances and nonces.
    /// Assumes the transaction has already been validated.
    pub fn apply_tx(&mut self, tx: &Transaction) {
        // ====================================================================
        // TODO 6: Implement applying a transaction to the state.
        // 1. Subtract the amount from the sender's balance.
        // 2. Add the amount to the receiver's balance.
        // 3. Increment the sender's nonce.
        // Hint: Use `self.balances.entry(...).or_insert(0)` to modify balances.
        // ====================================================================
        let sender_key_bytes = tx.sender.to_bytes();
        let mut sender_key_array = [0u8; 32];
        sender_key_array.copy_from_slice(&sender_key_bytes);
        match &tx.data {
            TransactionData::Transfer { receiver, amount } => {
                let receiver_key_bytes = receiver.to_bytes();
                let mut receiver_key_array = [0u8; 32];
                receiver_key_array.copy_from_slice(&receiver_key_bytes);
                *self.balances.entry(sender_key_array).or_insert(0) -= *amount;
                *self.balances.entry(receiver_key_array).or_insert(0) += *amount;
                *self.nonces.entry(sender_key_array).or_insert(0) += 1;
            }
            TransactionData::Stake { amount } => {
                *self.balances.entry(sender_key_array).or_insert(0) -= *amount;
                *self.stakes.entry(sender_key_array).or_insert(0) += *amount;
                *self.nonces.entry(sender_key_array).or_insert(0) += 1;
            }
        }
    }

    pub fn get_expected_validator(&self, next_block_height: u64) -> Option<VerifyingKey> {
        if self.stakes.is_empty() {
            return None;
        }
        let total_stake: u64 = self.stakes.values().sum();
        let winning_ticket = next_block_height % total_stake;
        let mut cumulative_stake = 0;
        for (key_bytes, stake) in &self.stakes {
            cumulative_stake += *stake;
            if winning_ticket < cumulative_stake {
                let mut key_array = [0u8; 32];
                key_array.copy_from_slice(key_bytes);
                return Some(VerifyingKey::from_bytes(&key_array).unwrap());
            }
        }
        None
    }

    pub fn compute_state_root(&self) -> [u8; 32] {
        let mut balances_vec: Vec<([u8; 32], u64)> = self.balances.clone().into_iter().collect();
        balances_vec.sort_by_key(|(k, _)| *k);
        let mut hashes = balances_vec
            .into_iter()
            .map(|(k, v)| {
                let mut data: Vec<u8> = Vec::new();
                let mut key_array = [0u8; 32];
                data.extend_from_slice(&k);
                data.extend_from_slice(&v.to_bytes());
                key_array.copy_from_slice(&data.hash());
                key_array
            })
            .collect::<Vec<[u8; 32]>>();
        while hashes.len() > 1 {
            let mut next_level: Vec<[u8; 32]> = Vec::new();
            for i in (0..hashes.len()).step_by(2) {
                let left = hashes[i];
                let right = if i + 1 < hashes.len() { hashes[i + 1] } else { left };
                let mut combined = Vec::new();
                combined.extend_from_slice(&left);
                combined.extend_from_slice(&right);
                let mut hash_array = [0u8; 32];
                hash_array.copy_from_slice(&combined.hash());
                next_level.push(hash_array);
            }
            hashes = next_level;
        }
        hashes
            .last()
            .cloned()
            .unwrap_or([0u8; 32]) // Placeholder: In a real implementation, this would be a Merkle root of the state.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{ SigningKey, Signer };
    use rand::rngs::OsRng;
    use crate::traits::Hashable;

    // Helper to create a signed transaction
    fn create_tx(
        sender: &SigningKey,
        receiver: &VerifyingKey,
        amount: u64,
        sequence: u64
    ) -> Transaction {
        let mut tx = Transaction {
            sender: sender.verifying_key(),
            data: TransactionData::Transfer {
                receiver: *receiver,
                amount,
            },
            sequence,
            signature: None,
        };
        let hash = tx.hash();
        tx.signature = Some(sender.sign(&hash[..]));
        tx
    }

    #[test]
    fn test_initial_state() {
        let state = State::new();
        let mut csprng = OsRng;
        let keypair = SigningKey::generate(&mut csprng);

        assert_eq!(state.get_balance(&keypair.verifying_key()), 0);
        assert_eq!(state.get_nonce(&keypair.verifying_key()), 0);
    }

    #[test]
    fn test_apply_valid_transaction() {
        let mut csprng = OsRng;
        let sender_keypair = SigningKey::generate(&mut csprng);
        let receiver_keypair = SigningKey::generate(&mut csprng);

        let mut state = State::new();

        // Give sender some initial balance (Uncomment after adding `balances` field)
        state.balances.insert(sender_keypair.verifying_key().to_bytes(), 100);

        let tx = create_tx(&sender_keypair, &receiver_keypair.verifying_key(), 50, 0);

        assert!(state.is_valid_tx(&tx));
        state.apply_tx(&tx);
        assert_eq!(state.get_balance(&sender_keypair.verifying_key()), 50);
        assert_eq!(state.get_balance(&receiver_keypair.verifying_key()), 50);
        assert_eq!(state.get_nonce(&sender_keypair.verifying_key()), 1);
    }

    #[test]
    fn test_insufficient_funds() {
        let mut csprng = OsRng;
        let sender_keypair = SigningKey::generate(&mut csprng);
        let receiver_keypair = SigningKey::generate(&mut csprng);

        let mut state = State::new();

        // Give sender ONLY 10 coins
        // state.balances.insert(sender_keypair.verifying_key().to_bytes(), 10);

        // Try to send 50
        let tx = create_tx(&sender_keypair, &receiver_keypair.verifying_key(), 50, 0);

        // Uncomment after implementing is_valid_tx
        // assert!(!state.is_valid_tx(&tx), "Transaction should be invalid due to insufficient funds");
    }

    #[test]
    fn test_invalid_nonce() {
        let mut csprng = OsRng;
        let sender_keypair = SigningKey::generate(&mut csprng);
        let receiver_keypair = SigningKey::generate(&mut csprng);

        let mut state = State::new();
        // state.balances.insert(sender_keypair.verifying_key().to_bytes(), 100);

        // Create tx with sequence 1, but expected nonce is 0
        let tx = create_tx(&sender_keypair, &receiver_keypair.verifying_key(), 50, 1);

        // Uncomment after implementing is_valid_tx
        // assert!(!state.is_valid_tx(&tx), "Transaction should be invalid due to wrong nonce");
    }

    #[test]
    fn test_invalid_signature() {
        let mut csprng = OsRng;
        let sender_keypair = SigningKey::generate(&mut csprng);
        let receiver_keypair = SigningKey::generate(&mut csprng);

        let mut state = State::new();
        // state.balances.insert(sender_keypair.verifying_key().to_bytes(), 100);

        let mut tx = create_tx(&sender_keypair, &receiver_keypair.verifying_key(), 50, 0);

        // Tamper with the transaction amount AFTER it was signed
        if let TransactionData::Transfer { amount, .. } = &mut tx.data {
            *amount = 100;
        }

        // Uncomment after implementing is_valid_tx
        assert!(
            !state.is_valid_tx(&tx),
            "Transaction should be invalid due to tampered data/bad signature"
        );
    }

    #[test]
    fn test_validator_selection() {
        let mut csprng = OsRng;
        let val1 = SigningKey::generate(&mut csprng);
        let val2 = SigningKey::generate(&mut csprng);
        let val3 = SigningKey::generate(&mut csprng);

        let mut state = State::new();

        // No validators yet
        assert!(state.get_expected_validator(1).is_none());

        // Add stakes
        state.stakes.insert(val1.verifying_key().to_bytes(), 100);
        state.stakes.insert(val2.verifying_key().to_bytes(), 200);
        state.stakes.insert(val3.verifying_key().to_bytes(), 300);

        // Total stake is 600.
        // Because HashMap iteration order is random, we can't easily assert *which* specific
        // validator wins a specific block height unless we sort the keys in `get_expected_validator`.
        // But we CAN assert that it always returns *some* valid validator.

        let winner_block_1 = state.get_expected_validator(1).unwrap();
        assert!(
            winner_block_1 == val1.verifying_key() ||
                winner_block_1 == val2.verifying_key() ||
                winner_block_1 == val3.verifying_key()
        );

        // If you implemented the sorting logic in `get_expected_validator` (as hinted in Chapter 4),
        // you can write deterministic assertions here! For example, if sorted by bytes:
        // let mut keys = vec![val1.verifying_key().to_bytes(), val2.verifying_key().to_bytes(), val3.verifying_key().to_bytes()];
        // keys.sort();
        // ... then you can calculate exactly who should win block 1, 2, 3, etc.
    }

    #[test]
    fn test_compute_state_root() {
        let mut state = State::new();
        let root1 = state.compute_state_root();

        let mut csprng = OsRng;
        let key1 = SigningKey::generate(&mut csprng).verifying_key().to_bytes();
        let key2 = SigningKey::generate(&mut csprng).verifying_key().to_bytes();

        state.balances.insert(key1, 100);
        let root2 = state.compute_state_root();

        // This will fail until you finish implementing compute_state_root!
        assert_ne!(root1, root2, "State root should change when state changes");

        state.balances.insert(key2, 200);
        let root3 = state.compute_state_root();
        assert_ne!(root2, root3, "State root should change when state changes");

        // Determinism check: insertion order shouldn't matter
        let mut state_clone = State::new();
        state_clone.balances.insert(key2, 200);
        state_clone.balances.insert(key1, 100);
        let root4 = state_clone.compute_state_root();

        assert_eq!(
            root3,
            root4,
            "State root should be deterministic regardless of insertion order"
        );
    }
}
