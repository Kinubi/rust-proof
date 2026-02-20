use sha2::{Digest, Sha256};
use ed25519_dalek::VerifyingKey;

/// A trait for converting a type into a flat array of bytes.
/// This is essential for hashing and network transmission.
pub trait ToBytes {
    /// Converts the type into a `Vec<u8>`.
    fn to_bytes(&self) -> Vec<u8>;
}

/// A trait for hashing a type.
pub trait Hashable {
    /// Returns the SHA-256 hash of the type as a 32-byte array.
    fn hash(&self) -> [u8; 32];
}

// ============================================================================
// TODO 1: Implement `ToBytes` for `u64`
// Hint: Use `self.to_be_bytes()` and convert it to a `Vec<u8>`.
// ============================================================================
impl ToBytes for u64 {
    fn to_bytes(&self) -> Vec<u8> {
        // YOUR CODE HERE
        unimplemented!("Implement ToBytes for u64")
    }
}

// ============================================================================
// TODO 2: Implement `ToBytes` for `String`
// Hint: A string is just a sequence of bytes. However, when we deserialize
// later, we need to know how long the string is.
// So, first write the length of the string as a `u64`, then write the actual bytes.
// ============================================================================
impl ToBytes for String {
    fn to_bytes(&self) -> Vec<u8> {
        // YOUR CODE HERE
        unimplemented!("Implement ToBytes for String")
    }
}

// ============================================================================
// TODO 3: Implement `ToBytes` for `Vec<T>` where `T` also implements `ToBytes`.
// Hint: This is your first complex trait bound!
// Like strings, write the length of the vector as a `u64` first.
// Then iterate over the items, call `.to_bytes()` on each, and append them.
// ============================================================================
impl<T: ToBytes> ToBytes for Vec<T> {
    fn to_bytes(&self) -> Vec<u8> {
        // YOUR CODE HERE
        unimplemented!("Implement ToBytes for Vec<T>")
    }
}

// ============================================================================
// TODO 3.5: Implement `ToBytes` for `[u8; 32]` and `VerifyingKey`.
// Hint: `[u8; 32]` is already bytes, just convert it to a `Vec<u8>`.
// Hint: `VerifyingKey` has a `.to_bytes()` method that returns `[u8; 32]`.
// ============================================================================
impl ToBytes for [u8; 32] {
    fn to_bytes(&self) -> Vec<u8> {
        // YOUR CODE HERE
        unimplemented!("Implement ToBytes for [u8; 32]")
    }
}

impl ToBytes for VerifyingKey {
    fn to_bytes(&self) -> Vec<u8> {
        // YOUR CODE HERE
        unimplemented!("Implement ToBytes for VerifyingKey")
    }
}

// ============================================================================
// TODO 4: Implement `Hashable` for ANY type `T` that implements `ToBytes`.
// Hint: This is called a "Blanket Implementation".
// 1. Call `self.to_bytes()`.
// 2. Create a new `Sha256` hasher: `let mut hasher = Sha256::new();`
// 3. Update the hasher with the bytes: `hasher.update(&bytes);`
// 4. Finalize the hash and convert it to a `[u8; 32]`.
// ============================================================================
impl<T: ToBytes> Hashable for T {
    fn hash(&self) -> [u8; 32] {
        // YOUR CODE HERE
        unimplemented!("Implement Hashable for T: ToBytes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_u64_to_bytes() {
        let num: u64 = 42;
        let bytes = num.to_bytes();
        assert_eq!(bytes, vec![0, 0, 0, 0, 0, 0, 0, 42]);
    }

    #[test]
    fn test_string_to_bytes() {
        let s = String::from("hello");
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
