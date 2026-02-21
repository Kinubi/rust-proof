use std::collections::HashMap;
use ed25519_dalek::VerifyingKey;
use crate::models::transaction::Transaction;

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
        if tx.amount > sender_balance {
            return false;
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
        let receiver_key_bytes = tx.receiver.to_bytes();
        let mut receiver_key_array = [0u8; 32];
        receiver_key_array.copy_from_slice(&receiver_key_bytes);
        *self.balances.entry(sender_key_array).or_insert(0) -= tx.amount;
        *self.balances.entry(receiver_key_array).or_insert(0) += tx.amount;
        *self.nonces.entry(sender_key_array).or_insert(0) += 1;
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
            receiver: *receiver,
            amount,
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
        tx.amount = 100;

        // Uncomment after implementing is_valid_tx
        // assert!(!state.is_valid_tx(&tx), "Transaction should be invalid due to tampered data/bad signature");
    }
}
