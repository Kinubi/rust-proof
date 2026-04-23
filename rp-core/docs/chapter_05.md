# Chapter 5: Persistent Storage and Merkle Trees

Right now, our node is completely amnesiac. If you stop the process (Ctrl+C) and restart it, the entire blockchain and all account balances are wiped out. 

A real blockchain must persist its data to disk so it can recover its state after a crash or restart. Furthermore, we need a way to cryptographically prove the state of the ledger without having to download the entire history of the chain.

## Domain Knowledge: Key-Value Stores and Merkle Trees

### 1. Key-Value Databases
Relational databases (like PostgreSQL) are too heavy and rigid for blockchains. Instead, blockchains use embedded Key-Value (KV) stores. You provide a byte array as a key, and it stores a byte array as a value.
*   **Bitcoin/Ethereum:** Use LevelDB or RocksDB (written in C++).
*   **Our Node:** We will use `sled`, a modern, high-performance embedded database written entirely in pure Rust!

### 2. Merkle Trees (State Roots)
How do you prove to a "light client" (like a mobile phone) that Alice has 50 coins, without making the phone download 500GB of block history?
You use a **Merkle Tree**.
*   You take all the account balances in the state.
*   You hash them in pairs, then hash the resulting hashes in pairs, all the way up until you have a single 32-byte hash: the **State Root**.
*   This State Root is included in the `Block` header.
*   If a single balance changes, the State Root changes completely. This allows anyone to verify the exact state of the network at any given block.

## The Rust Focus: Traits as Interfaces and Dynamic Dispatch

This chapter focuses heavily on **Abstraction**. We don't want our `Blockchain` struct to be hardcoded to use `sled`. What if we want to switch to `rocksdb` later? What if we want to use an in-memory mock database for unit testing?

We will define a `Storage` trait. Our `Blockchain` will then hold a `Box<dyn Storage>`—a dynamically dispatched trait object. This is Rust's version of "Dependency Injection" or "Programming to an Interface".

## Your Task: Implementing Persistence

We will create a new module: `storage.rs`.

### Step 1: The `Storage` Trait
1.  Open `src/storage.rs`.
2.  Define a `Storage` trait with methods to save and retrieve blocks and state.
3.  Because we are using `Box<dyn Storage>`, the trait must be "Object Safe" (e.g., no generic methods, no `Self` return types).

### Step 2: The `SledStorage` Implementation
1.  Implement the `Storage` trait for a new struct called `SledStorage` that wraps a `sled::Db` instance.
2.  You will need to serialize your `Block` to bytes to save it. (You already have `ToBytes`! But you might need a way to deserialize it back from bytes. For this chapter, we will provide a simple `FromBytes` trait or just use a serialization crate like `bincode` to save time, but let's stick to our raw bytes if possible, or just focus on the *saving* part first).

### Step 3: Merkle Root Calculation
1.  Open `src/state.rs`.
2.  Implement a `compute_state_root(&self) -> [u8; 32]` method.
3.  *Algorithm:* Extract all `(VerifyingKey, balance)` pairs from the `balances` HashMap. Sort them by the key (to ensure determinism). Hash each pair. Then hash the hashes together until you get one root hash. (For a simplified version, just concatenate all sorted keys and balances into one giant byte vector and hash it once).

### Step 4: Wiring it into the Blockchain
1.  Open `src/blockchain.rs`.
2.  Update the `Blockchain` struct to include `storage: Box<dyn Storage>`.
3.  Update `Blockchain::new()` to accept a `Box<dyn Storage>` as an argument.
4.  In `add_block`, after validating the block and updating the state, call `self.storage.save_block(&block)` and `self.storage.save_state_root(block.height, self.state.compute_state_root())`.

### Hints for `Storage` (in `src/storage.rs`):
*   **`sled` basics:** `let db = sled::open("my_db").unwrap(); db.insert(key_bytes, value_bytes).unwrap();`
*   **Trait Objects:** `pub struct Blockchain { storage: Box<dyn Storage> }`. When creating it: `Blockchain::new(Box::new(SledStorage::new("data")) )`.
*   **Error Handling:** Database operations can fail (e.g., disk full). Your trait methods should return `Result<(), String>` or a custom error type.

Go to `Cargo.toml` and add `sled = "0.34"`. Then create `src/storage.rs` and complete the `TODO`s. Run `cargo check` to verify your implementation.