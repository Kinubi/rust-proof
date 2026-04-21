use crate::traits::{ Hashable, ToBytes };
use ed25519_dalek::{ Signature, VerifyingKey };
use serde::{ Deserialize, Serialize };
use std::cmp::Ordering;
use crate::models::slashing::SlashProof;

/// A transaction represents a transfer of value from one account to another.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    /// The public key of the sender.
    pub sender: VerifyingKey,
    /// The public key of the receiver.
    pub data: TransactionData,
    /// A unique number to prevent replay attacks (like a nonce).
    pub sequence: u64,
    /// The fee paid to the validator for including this transaction.
    pub fee: u64,
    /// The cryptographic signature proving the sender authorized this transaction.
    pub signature: Option<Signature>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionData {
    Transfer {
        receiver: VerifyingKey,
        amount: u64,
    },
    Stake {
        amount: u64,
    },
    Unstake {
        amount: u64,
    },
    Slash {
        proof: SlashProof,
    },
}

#[derive(Debug, Clone)]
pub struct UnstakeRequest {
    pub amount: u64,
    pub unlock_slot: u64,
}

impl ToBytes for Transaction {
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.sender.to_bytes());
        match &self.data {
            TransactionData::Transfer { receiver, amount } => {
                bytes.extend_from_slice(&[0u8]); // 0 for Transfer
                bytes.extend_from_slice(&receiver.to_bytes());
                bytes.extend_from_slice(&amount.to_be_bytes());
            }
            TransactionData::Stake { amount } => {
                bytes.extend_from_slice(&[1u8]); // 1 for Stake
                bytes.extend_from_slice(&amount.to_be_bytes());
            }
            TransactionData::Unstake { amount } => {
                bytes.extend_from_slice(&[2u8]); // 2 for Unstake
                bytes.extend_from_slice(&amount.to_be_bytes());
            }
            TransactionData::Slash { proof } => {
                bytes.extend_from_slice(&[3u8]); // 3 for Slash
                bytes.extend_from_slice(&proof.to_bytes());
            }
        }
        bytes.extend_from_slice(&self.sequence.to_be_bytes());
        bytes.extend_from_slice(&self.fee.to_be_bytes());
        bytes
    }
}

impl Transaction {
    /// Verifies that the transaction signature is valid.
    pub fn is_valid(&self) -> bool {
        if let Some(signature) = &self.signature {
            let hash = self.hash();
            self.sender.verify_strict(&hash[..], signature).is_ok()
        } else {
            return false;
        }
    }
}

impl PartialEq for Transaction {
    fn eq(&self, other: &Self) -> bool {
        self.hash() == other.hash()
    }
}

impl Eq for Transaction {}

impl PartialOrd for Transaction {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Transaction {
    fn cmp(&self, other: &Self) -> Ordering {
        // Compare fees (descending)
        match self.fee.cmp(&other.fee).reverse() {
            Ordering::Equal => {
                // If fees are equal, compare hashes (ascending)
                self.hash().cmp(&other.hash())
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{ SigningKey, Signer };
    use rand::rngs::OsRng;

    #[test]
    fn test_transaction_signing_and_verification() {
        let mut csprng = OsRng;
        let sender_keypair = SigningKey::generate(&mut csprng);
        let receiver_keypair = SigningKey::generate(&mut csprng);

        let mut tx = Transaction {
            sender: sender_keypair.verifying_key(),
            data: TransactionData::Transfer {
                receiver: receiver_keypair.verifying_key(),
                amount: 100,
            },
            sequence: 1,
            fee: 10,
            signature: None,
        };

        // Hash the transaction (without signature)
        let hash = tx.hash();

        // Sign the hash
        let signature = sender_keypair.sign(&hash[..]);
        tx.signature = Some(signature);

        // Verify
        assert!(tx.is_valid());
    }
}
