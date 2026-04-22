use ed25519_dalek::VerifyingKey;
use crate::models::slashing::SlashProof;
use crate::models::transaction::{ Transaction, TransactionData, UnstakeRequest };
use crate::traits::{ ToBytes, FromBytes, Hashable };
use crate::errors::SlashError;
use alloc::vec::Vec;
use alloc::vec;
use alloc::collections::BTreeMap;

pub const EPOCH_LENGTH: u64 = 32; // Example: 32 slots per epoch
pub const UNSTAKE_LOCK_EPOCHS: u64 = 2; // Example: unstaked funds unlock after 2 epochs
/// The State represents the current balances and sequence numbers of all accounts.
#[derive(Debug, Clone, Default)]
pub struct State {
    pub balances: BTreeMap<[u8; 32], u64>,
    pub nonces: BTreeMap<[u8; 32], u64>,
    pub stakes: BTreeMap<[u8; 32], u64>,
    pub unstaking: BTreeMap<[u8; 32], Vec<UnstakeRequest>>,
}

impl State {
    pub fn new() -> Self {
        Self {
            balances: BTreeMap::new(),
            nonces: BTreeMap::new(),
            stakes: BTreeMap::new(),
            unstaking: BTreeMap::new(),
        }
    }

    /// Returns the balance of an account.
    pub fn get_balance(&self, account: &VerifyingKey) -> u64 {
        let key_bytes = account.to_bytes();
        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(&key_bytes);
        *self.balances.get(&key_array).unwrap_or(&0)
    }

    /// Returns the next expected sequence number for an account.
    pub fn get_nonce(&self, account: &VerifyingKey) -> u64 {
        let key_bytes = account.to_bytes();
        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(&key_bytes);
        *self.nonces.get(&key_array).unwrap_or(&0)
    }

    /// Validates a transaction against the current state.
    /// Returns true if the transaction is valid, false otherwise.
    pub fn is_valid_tx(&self, tx: &Transaction) -> bool {
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
            TransactionData::Unstake { amount } => {
                let sender_key_bytes = tx.sender.to_bytes();
                let mut sender_key_array = [0u8; 32];
                sender_key_array.copy_from_slice(&sender_key_bytes);
                let current_stake = *self.stakes.get(&sender_key_array).unwrap_or(&0);
                if current_stake < *amount {
                    return false;
                }
                // Additional checks for unlock_slot can be added here if needed
            }
            TransactionData::Slash { proof } => {
                // Slash transactions should be validated separately, but we can check the proof here as well
                if !proof.validate().is_ok() {
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
    pub fn apply_tx(&mut self, tx: &Transaction, current_slot: u64) {
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
            TransactionData::Unstake { amount } => {
                *self.stakes.entry(sender_key_array).or_insert(0) -= *amount;
                self.unstaking
                    .entry(sender_key_array)
                    .or_insert(vec![])
                    .push(UnstakeRequest {
                        amount: *amount,
                        unlock_slot: current_slot + UNSTAKE_LOCK_EPOCHS * EPOCH_LENGTH, // Example: unlock after 2 epochs
                    });
                *self.nonces.entry(sender_key_array).or_insert(0) += 1;
            }
            TransactionData::Slash { proof } => {
                // Slash transactions should be applied separately, but we can apply the slash here as well
                let _ = self.apply_slash(proof.clone());
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

    pub fn apply_slash(&mut self, proof: SlashProof) -> Result<(), SlashError> {
        let _ = proof.validate()?;

        let validator_key_bytes = proof.validator.to_bytes();
        let mut validator_key_array = [0u8; 32];
        validator_key_array.copy_from_slice(&validator_key_bytes);
        self.stakes.remove(&validator_key_array);
        self.unstaking.remove(&validator_key_array);
        Ok(())
    }
}

impl ToBytes for State {
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        // Helper to write a map of [u8; 32] -> u64
        let write_u64_map = |map: &BTreeMap<[u8; 32], u64>, out: &mut Vec<u8>| {
            out.extend_from_slice(&(map.len() as u64).to_be_bytes());
            for (k, v) in map {
                out.extend_from_slice(k);
                out.extend_from_slice(&v.to_be_bytes());
            }
        };

        write_u64_map(&self.balances, &mut bytes);
        write_u64_map(&self.nonces, &mut bytes);
        write_u64_map(&self.stakes, &mut bytes);

        // Write unstaking map
        bytes.extend_from_slice(&(self.unstaking.len() as u64).to_be_bytes());
        for (account, requests) in &self.unstaking {
            bytes.extend_from_slice(account);
            bytes.extend_from_slice(&(requests.len() as u64).to_be_bytes());
            for req in requests {
                bytes.extend_from_slice(&req.amount.to_be_bytes());
                bytes.extend_from_slice(&req.unlock_slot.to_be_bytes());
            }
        }

        bytes
    }
}

impl FromBytes for State {
    fn from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        let mut state = State::new();
        let mut i = 0;

        fn read_u64(bytes: &[u8], i: &mut usize) -> Result<u64, &'static str> {
            if *i + 8 > bytes.len() {
                return Err("Unexpected EOF reading u64");
            }
            let val = u64::from_be_bytes(bytes[*i..*i + 8].try_into().unwrap());
            *i += 8;
            Ok(val)
        }

        fn read_key(bytes: &[u8], i: &mut usize) -> Result<[u8; 32], &'static str> {
            if *i + 32 > bytes.len() {
                return Err("Unexpected EOF reading key");
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes[*i..*i + 32]);
            *i += 32;
            Ok(key)
        }

        fn read_u64_map(
            bytes: &[u8],
            i: &mut usize,
            map: &mut BTreeMap<[u8; 32], u64>
        ) -> Result<(), &'static str> {
            let len = read_u64(bytes, i)?;
            for _ in 0..len {
                let k = read_key(bytes, i)?;
                let v = read_u64(bytes, i)?;
                map.insert(k, v);
            }
            Ok(())
        }

        read_u64_map(bytes, &mut i, &mut state.balances)?;
        read_u64_map(bytes, &mut i, &mut state.nonces)?;
        read_u64_map(bytes, &mut i, &mut state.stakes)?;

        let unstaking_len = read_u64(bytes, &mut i)?;
        for _ in 0..unstaking_len {
            let k = read_key(bytes, &mut i)?;
            let req_len = read_u64(bytes, &mut i)?;
            let mut requests = Vec::with_capacity(req_len as usize);
            for _ in 0..req_len {
                let amount = read_u64(bytes, &mut i)?;
                let unlock_slot = read_u64(bytes, &mut i)?;
                requests.push(UnstakeRequest { amount, unlock_slot });
            }
            state.unstaking.insert(k, requests);
        }

        Ok(state)
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
            fee: 10,
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
        let current_slot = 1; // Example current slot
        state.apply_tx(&tx, current_slot);
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
        state.balances.insert(sender_keypair.verifying_key().to_bytes(), 10);

        // Try to send 50
        let tx = create_tx(&sender_keypair, &receiver_keypair.verifying_key(), 50, 0);

        // Uncomment after implementing is_valid_tx
        assert!(!state.is_valid_tx(&tx), "Transaction should be invalid due to insufficient funds");
    }

    #[test]
    fn test_invalid_nonce() {
        let mut csprng = OsRng;
        let sender_keypair = SigningKey::generate(&mut csprng);
        let receiver_keypair = SigningKey::generate(&mut csprng);

        let mut state = State::new();
        state.balances.insert(sender_keypair.verifying_key().to_bytes(), 100);

        // Create tx with sequence 1, but expected nonce is 0
        let tx = create_tx(&sender_keypair, &receiver_keypair.verifying_key(), 50, 1);

        // Uncomment after implementing is_valid_tx
        assert!(!state.is_valid_tx(&tx), "Transaction should be invalid due to wrong nonce");
    }

    #[test]
    fn test_invalid_signature() {
        let mut csprng = OsRng;
        let sender_keypair = SigningKey::generate(&mut csprng);
        let receiver_keypair = SigningKey::generate(&mut csprng);

        let mut state = State::new();
        state.balances.insert(sender_keypair.verifying_key().to_bytes(), 100);

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
