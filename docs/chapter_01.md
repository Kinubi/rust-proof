# Chapter 1: Cryptography, Serialization, and Core Data Structures

Welcome to `rust-proof`. This project will take you through building a Proof of Stake (PoS) blockchain from scratch, focusing heavily on advanced Rust concepts: the borrow checker, complex trait bounds, and multithreading.

We are keeping dependencies to an absolute minimum. We will use `sha2` for hashing and `ed25519-dalek` for digital signatures. Everything else, including serializing our structs into bytes so they can be hashed, we will build ourselves using Rust traits.

## Domain Knowledge: The Primitives of a Blockchain

A blockchain is a distributed, append-only ledger. To ensure nobody tampers with the ledger, we rely on two cryptographic primitives:

### 1. Cryptographic Hashing (SHA-256)
A hash function takes an arbitrary amount of data (bytes) and deterministically produces a fixed-size output (a 32-byte array for SHA-256).
* **Deterministic:** The same input always produces the same output.
* **Collision Resistant:** It is computationally infeasible to find two different inputs that produce the same output.
* **One-Way:** You cannot reverse the hash to find the original input.
* **Avalanche Effect:** Changing a single bit in the input completely changes the output hash.

In our blockchain, every `Block` and `Transaction` will have a unique Hash. This hash acts as its unique identifier (ID). If anyone tries to alter a transaction, its hash changes, invalidating it.

### 2. Digital Signatures (Ed25519)
To prove that you own the funds you are trying to spend, you must sign your transaction.
* **Keypair:** You generate a Public Key (your address) and a Private Key (your secret password).
* **Signing:** You use your Private Key to encrypt the *Hash* of your transaction. This produces a Signature.
* **Verification:** Anyone can use your Public Key, the Signature, and the Transaction Hash to verify that the owner of the Private Key authorized this exact transaction.

### 3. Serialization (The `ToBytes` Trait)
Before we can hash a `Transaction` or a `Block`, we must convert its Rust struct representation into a flat array of bytes (`Vec<u8>`).
Instead of using a heavy crate like `serde`, we will define our own `ToBytes` trait. This is a fantastic exercise in understanding how data is laid out in memory and how to use traits to define common behavior across different types.

## Your First Task: The `ToBytes` and `Hashable` Traits

We need a way to convert any struct into bytes, and then hash those bytes.

1. Open `src/traits.rs`.
2. I have scaffolded the `ToBytes` and `Hashable` traits.
3. Your job is to implement `ToBytes` for basic Rust types (`u64`, `String`, `Vec<T>`).
4. Then, implement `Hashable` for any type `T` that implements `ToBytes`. This is your first encounter with **Blanket Implementations** and **Trait Bounds**.

### Hints for `ToBytes`:
* To convert a `u64` to bytes, look at the `u64::to_be_bytes()` method. It returns an array `[u8; 8]`. You can extend a `Vec<u8>` with this array.
* To convert a `String` to bytes, look at `String::as_bytes()`. However, when deserializing later, we need to know how long the string is. So, a common pattern is to first write the length of the string (as a `u64`), followed by the actual string bytes.
* To convert a `Vec<T>` to bytes, you must ensure `T` also implements `ToBytes`. You write the length of the vector first, then iterate over the items and append their bytes.

### Hints for `Hashable`:
* You want to write: `impl<T: ToBytes> Hashable for T { ... }`
* Inside the `hash` method, call `self.to_bytes()`.
* Then use the `sha2` crate to hash those bytes. You'll need to create a new `Sha256` hasher, update it with your bytes, and finalize it. The `sha2` crate documentation or examples online usually show this as:
  ```rust
  let mut hasher = Sha256::new();
  hasher.update(&bytes);
  let result = hasher.finalize();
  // result is a GenericArray, you can convert it to [u8; 32] using .into()
  ```

### Hints for `Transaction` (in `src/models/transaction.rs`):
* **`ToBytes`:** Create a mutable `Vec<u8>`. Call `.to_bytes()` on `sender`, `receiver`, `amount`, and `sequence`, and extend your vector with these bytes. **Do not** include the `signature` field, as the signature is generated *from* this hash.
* **`is_valid`:** 
  1. Extract the signature. If `self.signature` is `None`, return `false`.
  2. Get the hash of the transaction by calling `self.hash()`. (This works because `Transaction` implements `ToBytes`, and we wrote a blanket impl for `Hashable`!).
  3. Use the `ed25519_dalek` API to verify: `self.sender.verify_strict(&hash, &sig).is_ok()`.

### Hints for `Block` (in `src/models/block.rs`):
* **`ToBytes`:** Similar to `Transaction`, create a `Vec<u8>` and extend it with the bytes of `height`, `previous_hash`, `validator`, and `transactions`. Again, omit the `signature`.
* **`is_valid`:** 
  1. Extract the signature. If `self.signature` is `None`, return `false`.
  2. Hash the block using `self.hash()`.
  3. Verify the signature using the validator's public key: `self.validator.verify_strict(&hash, &sig).is_ok()`.

Go to `src/traits.rs`, `src/models/transaction.rs`, and `src/models/block.rs` and complete the `TODO`s. Run `cargo test` to verify your implementation.
