use alloc::vec::Vec;
use serde::{ Deserialize, Deserializer, Serializer, de::Error as DeError };

pub use p256::ecdsa::signature::{ Signer, Verifier };
pub use p256::ecdsa::{ Signature, SigningKey, VerifyingKey };

pub const PUBLIC_KEY_SIZE: usize = 33;
pub type PublicKeyBytes = [u8; PUBLIC_KEY_SIZE];

pub fn verifying_key_to_bytes(key: &VerifyingKey) -> PublicKeyBytes {
    let encoded = key.to_encoded_point(true);
    let mut bytes = [0u8; PUBLIC_KEY_SIZE];
    bytes.copy_from_slice(encoded.as_bytes());
    bytes
}

pub fn verifying_key_bytes(key: &VerifyingKey) -> Vec<u8> {
    verifying_key_to_bytes(key).to_vec()
}

pub fn verifying_key_from_bytes(bytes: &PublicKeyBytes) -> Result<VerifyingKey, &'static str> {
    VerifyingKey::from_sec1_bytes(bytes).map_err(|_| "invalid verifying key")
}

pub fn signing_key_from_bytes(bytes: &[u8; 32]) -> Result<SigningKey, &'static str> {
    SigningKey::from_slice(bytes).map_err(|_| "invalid signing key")
}

pub fn genesis_verifying_key() -> VerifyingKey {
    signing_key_from_bytes(&[1u8; 32])
        .expect("fixed genesis signing key should be valid")
        .verifying_key()
        .clone()
}

pub mod serde_verifying_key {
    use super::*;

    pub fn serialize<S>(key: &VerifyingKey, serializer: S) -> Result<S::Ok, S::Error>
        where S: Serializer
    {
        serializer.serialize_bytes(&verifying_key_to_bytes(key))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<VerifyingKey, D::Error>
        where D: Deserializer<'de>
    {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        let len = bytes.len();
        let key_bytes: PublicKeyBytes = bytes
            .try_into()
            .map_err(|_| D::Error::invalid_length(len, &"33-byte compressed P-256 public key"))?;

        verifying_key_from_bytes(&key_bytes).map_err(D::Error::custom)
    }
}
