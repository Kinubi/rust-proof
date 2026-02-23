use crate::{ models::block::Block, traits::ToBytes };
use ed25519_dalek::VerifyingKey;
use serde::{ Deserialize, Serialize };
use crate::traits::Hashable;

/// A cryptographic proof that a validator signed two different blocks for the same slot.
/// This is a slashable offense (equivocation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlashProof {
    pub validator: VerifyingKey,
    pub block_a: Block,
    pub block_b: Block,
}

impl SlashProof {
    /// Verifies that the slash proof is valid.
    /// A valid proof must show that the same validator signed two different blocks
    /// for the exact same slot.
    pub fn is_valid(&self) -> bool {
        if self.block_a.slot != self.block_b.slot {
            println!("Invalid proof: blocks are from different slots");
            return false;
        }
        if self.block_a.hash() == self.block_b.hash() {
            println!("Invalid proof: blocks are identical");
            return false;
        }
        if self.block_a.validator != self.validator {
            println!("Invalid proof: block_a validator does not match proof validator");
            return false;
        }
        if self.block_b.validator != self.validator {
            println!("Invalid proof: block_b validator does not match proof validator");
            return false;
        }
        if !self.block_a.is_valid() {
            println!("Invalid proof: block_a signature is invalid");
            return false;
        }
        if !self.block_b.is_valid() {
            println!("Invalid proof: block_b signature is invalid");
            return false;
        }
        true
    }
}

impl ToBytes for SlashProof {
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.validator.to_bytes());
        bytes.extend_from_slice(&self.block_a.to_bytes());
        bytes.extend_from_slice(&self.block_b.to_bytes());
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{ SigningKey, Signer };
    use rand::rngs::OsRng;
    // Add tests here to verify that valid slash proofs pass and invalid ones fail!
    #[test]
    fn test_valid_slash_proof() {
        let mut csprng = OsRng;
        let validator_keypair = SigningKey::generate(&mut csprng);
        let mut block_a = Block {
            height: 2,
            slot: 1,
            previous_hash: [0; 32],
            validator: validator_keypair.verifying_key(),
            transactions: vec![],
            signature: None,
            slash_proofs: vec![],
            state_root: [0u8; 32],
        };
        let mut block_b = Block {
            height: 1,
            slot: 1,
            previous_hash: [0; 32],
            validator: validator_keypair.verifying_key(),
            transactions: vec![],
            signature: None,
            slash_proofs: vec![],
            state_root: [0u8; 32],
        };
        // Hash the block (without signature)
        let hash_a = block_a.hash();

        // Sign the hash
        let signature_a = validator_keypair.sign(&hash_a[..]);
        block_a.signature = Some(signature_a);

        // Hash the block (without signature)
        let hash_b = block_b.hash();

        // Sign the hash
        let signature_b = validator_keypair.sign(&hash_b[..]);
        block_b.signature = Some(signature_b);

        let proof = SlashProof {
            validator: validator_keypair.verifying_key(),
            block_a,
            block_b,
        };
        assert!(proof.is_valid());
    }
}
