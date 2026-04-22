use crate::models::slashing::SlashProof;
use crate::traits::{ Hashable, ToBytes };
use crate::models::transaction::Transaction;
use ed25519_dalek::{ Signature, VerifyingKey };
use alloc::vec::Vec;

use serde_derive::{ Serialize, Deserialize };

/// A block contains a list of transactions and a reference to the previous block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    /// The height of the block in the chain (0 for genesis).
    pub height: u64,
    /// The slot in which this block was proposed.
    pub slot: u64,
    /// The hash of the previous block.
    pub previous_hash: [u8; 32],
    /// The public key of the validator who forged this block.
    pub validator: VerifyingKey,
    /// The transactions included in this block.
    pub transactions: Vec<Transaction>,
    /// The signature of the validator proving they forged this block.
    pub signature: Option<Signature>,
    /// Slash transactions included in this block (if any).
    pub slash_proofs: Vec<SlashProof>,
    /// The state root after applying this block's transactions.
    pub state_root: [u8; 32],
}

impl ToBytes for Block {
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.height.to_be_bytes());
        bytes.extend_from_slice(&self.slot.to_bytes());
        bytes.extend_from_slice(&self.previous_hash);
        bytes.extend_from_slice(&self.validator.to_bytes());
        bytes.extend_from_slice(&self.transactions.to_bytes());
        bytes.extend_from_slice(&self.slash_proofs.to_bytes());
        bytes.extend_from_slice(&self.state_root);
        bytes
    }
}

impl Block {
    /// Verifies that the block signature is valid.
    pub fn is_valid(&self) -> bool {
        if let Some(signature) = &self.signature {
            let hash = self.hash();
            self.validator.verify_strict(&hash[..], signature).is_ok()
        } else {
            return false;
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockNode {
    pub block: Block,
    pub children: Vec<[u8; 32]>, // Hashes of child blocks
}

impl ToBytes for BlockNode {
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.block.to_bytes());
        for child in &self.children {
            bytes.extend_from_slice(child);
        }
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{ SigningKey, Signer };
    use rand::rngs::OsRng;
    use alloc::vec;

    #[test]
    fn test_block_signing_and_verification() {
        let mut csprng = OsRng;
        let validator_keypair = SigningKey::generate(&mut csprng);

        let mut block = Block {
            height: 1,
            slot: 1,
            previous_hash: [0; 32],
            validator: validator_keypair.verifying_key(),
            transactions: vec![],
            signature: None,
            slash_proofs: vec![],
            state_root: [0; 32],
        };

        // Hash the block (without signature)
        let hash = block.hash();

        // Sign the hash
        let signature = validator_keypair.sign(&hash[..]);
        block.signature = Some(signature);

        // Verify
        assert!(block.is_valid());
    }
}
