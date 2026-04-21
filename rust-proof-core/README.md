# rust-proof

A from-scratch implementation of a Proof of Stake (PoS) blockchain in Rust, designed as a learning exercise for advanced Rust concepts.

## Project Goals

This project is structured as a series of chapters. Each chapter introduces domain knowledge about blockchains and provides scaffolded code with `TODO`s for you to implement. The goal is to practice:

*   **Traits and Blanket Implementations:** Building a custom serialization framework (`ToBytes`) and hashing framework (`Hashable`).
*   **Memory Layout:** Understanding how data is represented in bytes.
*   **The Borrow Checker:** Managing state mutations, borrowing fields, and understanding when to clone vs. borrow.
*   **Concurrency and Message Passing:** Using `tokio` to build a multi-threaded node architecture with MPSC and oneshot channels.
*   **Networking (Upcoming):** Using `libp2p` to connect nodes together.

## Architecture Overview

The final product will be a fully functional, multi-threaded blockchain node. For a detailed breakdown of the final architecture, features, and a system diagram, please see [docs/architecture.md](docs/architecture.md).

### 1. Core Data Structures (`src/models/`)
*   **`Transaction`**: Represents a transfer of value. Contains sender, receiver, amount, sequence (nonce), and a cryptographic signature (Ed25519).
*   **`Block`**: Batches transactions together. Contains height, previous hash, validator public key, transactions, and a validator signature.

### 2. Cryptography & Serialization (`src/traits.rs`)
*   **`ToBytes`**: A custom trait to convert structs into a flat `Vec<u8>`.
*   **`Hashable`**: A blanket trait implementation that uses `sha2` to hash any type that implements `ToBytes`.

### 3. State Management (`src/state.rs` & `src/blockchain.rs`)
*   **`State`**: A state machine that tracks account balances and nonces using `HashMap`s. It validates and applies transactions.
*   **`Blockchain`**: Holds the chain of `Block`s, the current `State`, and the mempool (pending transactions).

### 4. Node Architecture (`src/node.rs`)
The node is designed using the Actor Model to handle concurrency safely.
*   **The State Manager (Actor):** A dedicated `tokio` task that owns the `Blockchain`. It is the *only* task allowed to mutate the state.
*   **Message Passing:** Other components (like a CLI, Miner, or Network listener) communicate with the State Manager by sending `NodeCommand`s over an MPSC channel.
*   **Return Channels:** When a component needs data back (e.g., "Get Latest Block"), it includes a `oneshot::Sender` in the command.

### 5. Networking (Future Chapter)
*   A `libp2p` swarm that handles peer discovery and gossiping transactions and blocks across the network.

## How to Use This Repository

1.  Start with `docs/chapter_01.md`. Read the domain knowledge and follow the instructions to complete the `TODO`s in the source code.
2.  Run `cargo test` frequently to check your progress.
3.  Move on to the next chapter when all tests pass.

## Dependencies

We keep dependencies minimal to maximize learning:
*   `ed25519-dalek`: For digital signatures.
*   `sha2`: For cryptographic hashing.
*   `rand`: For generating keypairs (mostly in tests).
*   `tokio`: For the async runtime and channels.
