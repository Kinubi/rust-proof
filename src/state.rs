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
    // YOUR CODE HERE
}

impl State {
    // ====================================================================
    // TODO 2: Implement `State::new()`.
    // Initialize the HashMaps.
    // ====================================================================
    pub fn new() -> Self {
        unimplemented!("Implement State::new")
    }

    /// Returns the balance of an account.
    // ====================================================================
    // TODO 3: Implement `get_balance`.
    // Return the balance if it exists, otherwise return 0.
    // ====================================================================
    pub fn get_balance(&self, account: &VerifyingKey) -> u64 {
        unimplemented!("Implement get_balance")
    }

    /// Returns the next expected sequence number for an account.
    // ====================================================================
    // TODO 4: Implement `get_nonce`.
    // Return the nonce if it exists, otherwise return 0.
    // ====================================================================
    pub fn get_nonce(&self, account: &VerifyingKey) -> u64 {
        unimplemented!("Implement get_nonce")
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
        unimplemented!("Implement state validation for a transaction")
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
        unimplemented!("Implement applying a transaction to the state")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{ SigningKey, Signer };
    use rand::rngs::OsRng;
    use crate::traits::Hashable;

    #[test]
    fn test_state_validation_and_application() {
        let mut csprng = OsRng;
        let sender_keypair = SigningKey::generate(&mut csprng);
        let receiver_keypair = SigningKey::generate(&mut csprng);

        let mut state = State::new();
        // Give sender some initial balance
        // state.balances.insert(*sender_keypair.verifying_key().as_bytes(), 100);

        let mut tx = Transaction {
            sender: sender_keypair.verifying_key(),
            receiver: receiver_keypair.verifying_key(),
            amount: 50,
            sequence: 0,
            signature: None,
        };
        let hash = tx.hash();
        tx.signature = Some(sender_keypair.sign(&hash));

        // Should be valid
        // assert!(state.is_valid_tx(&tx));

        // Apply it
        // state.apply_tx(&tx);

        // Check balances
        // assert_eq!(state.get_balance(&sender_keypair.verifying_key()), 50);
        // assert_eq!(state.get_balance(&receiver_keypair.verifying_key()), 50);
        // assert_eq!(state.get_nonce(&sender_keypair.verifying_key()), 1);

        // Try to replay the same transaction
        // assert!(!state.is_valid_tx(&tx)); // Sequence is now wrong
    }
}
