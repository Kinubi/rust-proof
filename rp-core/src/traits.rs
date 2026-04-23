use sha2::{ Digest, Sha256 };
use alloc::vec::Vec;

use crate::crypto::{ VerifyingKey, verifying_key_bytes };

/// A trait for converting a type into a flat array of bytes.
/// This is essential for hashing and network transmission.
pub trait ToBytes {
    /// Converts the type into a `Vec<u8>`.
    fn to_bytes(&self) -> Vec<u8>;
}

/// A trait for converting a flat array of bytes back into a type.
pub trait FromBytes: Sized {
    /// Converts a `&[u8]` back into the type.
    fn from_bytes(bytes: &[u8]) -> Result<Self, &'static str>;
}

/// A trait for hashing a type.
pub trait Hashable {
    /// Returns the SHA-256 hash of the type as a 32-byte array.
    fn hash(&self) -> [u8; 32];
}

impl ToBytes for u64 {
    fn to_bytes(&self) -> Vec<u8> {
        let bytes = self.to_be_bytes();
        bytes.to_vec()
    }
}

impl ToBytes for &'static str {
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        let len = self.len() as u64;
        bytes.extend_from_slice(&len.to_be_bytes());
        bytes.extend_from_slice(self.as_bytes());
        bytes
    }
}

impl<T: ToBytes> ToBytes for Vec<T> {
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        let len = self.len() as u64;
        bytes.extend_from_slice(&len.to_be_bytes());
        for item in self.iter() {
            bytes.extend_from_slice(&item.to_bytes());
        }
        bytes
    }
}

impl ToBytes for [u8; 32] {
    fn to_bytes(&self) -> Vec<u8> {
        self.to_vec()
    }
}

impl ToBytes for Vec<u8> {
    fn to_bytes(&self) -> Vec<u8> {
        self.clone()
    }
}

impl ToBytes for VerifyingKey {
    fn to_bytes(&self) -> Vec<u8> {
        verifying_key_bytes(self)
    }
}

impl<T: ToBytes> Hashable for T {
    fn hash(&self) -> [u8; 32] {
        let bytes = self.to_bytes();
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_u64_to_bytes() {
        let num: u64 = 42;
        let bytes = num.to_bytes();
        assert_eq!(bytes, vec![0, 0, 0, 0, 0, 0, 0, 42]);
    }

    #[test]
    fn test_string_to_bytes() {
        let s = "hello";
        let bytes = s.to_bytes();
        // Length of "hello" is 5. So first 8 bytes should be 5.
        let mut expected = vec![0, 0, 0, 0, 0, 0, 0, 5];
        expected.extend_from_slice(b"hello");
        assert_eq!(bytes, expected);
    }

    #[test]
    fn test_vec_to_bytes() {
        let v: Vec<u64> = vec![1, 2];
        let bytes = v.to_bytes();
        // Length of vec is 2.
        let mut expected = vec![0, 0, 0, 0, 0, 0, 0, 2];
        // First element is 1
        expected.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 1]);
        // Second element is 2
        expected.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 2]);
        assert_eq!(bytes, expected);
    }

    #[test]
    fn test_hashable_blanket_impl() {
        let num: u64 = 42;
        let hash = num.hash();
        // Just checking it doesn't panic and returns 32 bytes.
        assert_eq!(hash.len(), 32);
    }
}
