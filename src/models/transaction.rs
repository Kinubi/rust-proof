use crate::traits::{Hashable, ToBytes};
use ed25519_dalek::{Signature, VerifyingKey};

/// A transaction represents a transfer of value from one account to another.
#[derive(Debug, Clone)]
pub struct Transaction {
    /// The public key of the sender.
    pub sender: VerifyingKey,
    /// The public key of the receiver.
    pub receiver: VerifyingKey,
    /// The amount being transferred.
    pub amount: u64,
    /// A unique number to prevent replay attacks (like a nonce).
    pub sequence: u64,
    /// The cryptographic signature proving the sender authorized this transaction.
    pub signature: Option<Signature>,
}

// ============================================================================
// TODO 5: Implement `ToBytes` for `Transaction`.
// Hint: You need to convert the `sender`, `receiver`, `amount`, and `sequence`
// into bytes and append them together.
// Note: We DO NOT include the `signature` in the bytes we hash. Why?
// Because the signature is created BY hashing the transaction. If the signature
// was part of the hash, it would be a circular dependency!
// ============================================================================
impl ToBytes for Transaction {
    fn to_bytes(&self) -> Vec<u8> {
        // YOUR CODE HERE
        unimplemented!("Implement ToBytes for Transaction")
    }
}

impl Transaction {
    /// Verifies that the transaction signature is valid.
    pub fn is_valid(&self) -> bool {
        // ====================================================================
        // TODO 6: Implement signature verification.
        // 1. Check if the signature is `Some`. If `None`, return false.
        // 2. Hash the transaction (using `self.hash()`).
        // 3. Use `self.sender.verify_strict(...)` to check the signature against the hash.
        // ====================================================================
        unimplemented!("Implement signature verification")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{SigningKey, Signer};
    use rand::rngs::OsRng;

    #[test]
    fn test_transaction_signing_and_verification() {
        let mut csprng = OsRng;
        let sender_keypair = SigningKey::generate(&mut csprng);
        let receiver_keypair = SigningKey::generate(&mut csprng);

        let mut tx = Transaction {
            sender: sender_keypair.verifying_key(),
            receiver: receiver_keypair.verifying_key(),
            amount: 100,
            sequence: 1,
            signature: None,
        };

        // Hash the transaction (without signature)
        let hash = tx.hash();
        
        // Sign the hash
        let signature = sender_keypair.sign(&hash);
        tx.signature = Some(signature);

        // Verify
        assert!(tx.is_valid());
    }
}
