# Chapter 8: Networking & P2P Communication

Welcome to Chapter 8! Up until now, our blockchain has been running entirely on a single node. A blockchain isn't very useful if it can't communicate with other nodes in the network.

In this chapter, we will introduce **Networking** to allow multiple nodes to discover each other, share transactions, and broadcast new blocks. We will be using the industry-standard **libp2p** framework.

## Domain Knowledge

### 1. Peer-to-Peer (P2P) Networks
Blockchains operate on a P2P network. There is no central server or master node. Every node connects to a few other nodes (peers) and relays information. When a node receives a new transaction or block, it validates it and then "gossips" it to its connected peers. This ensures that all nodes eventually reach the same state.

### 2. libp2p & Network Protocols
Instead of writing raw TCP sockets, modern blockchains (like Ethereum 2.0, Polkadot, and Filecoin) use `libp2p`. It provides modular network stacks. We will use two key protocols from `libp2p`:
*   **Kademlia DHT (Distributed Hash Table):** Used for decentralized peer discovery. Nodes can find other nodes in the network without relying on a central directory.
*   **Gossipsub:** A highly efficient pub/sub protocol for broadcasting messages (like new `Transaction`s and `Block`s) to the entire network.

### 3. Chain Synchronization
When a new node joins, it needs to download the history of the blockchain. We will define a custom request/response protocol using `libp2p`'s `RequestResponse` behavior to allow nodes to ask peers for missing blocks (`GetBlocks`).

---

## Your Tasks

### Step 1: Define Network Messages
1. Open `src/network/message.rs`.
2. Define an enum `NetworkMessage` that implements `Serialize` and `Deserialize` (you will need to add `serde` and `serde_json` or `bincode` to your `Cargo.toml`).
3. Add variants for:
   - `Transaction(crate::models::transaction::Transaction)`
   - `Block(crate::models::block::Block)`
   - `GetBlocks { from_height: u64, to_height: u64 }` (For the Request/Response protocol)

### Step 2: Create the libp2p Network Behavior
1. Open `src/network/manager.rs`.
2. You will need to add `libp2p` to your `Cargo.toml` with features like `tokio`, `gossipsub`, `kad`, `tcp`, `noise`, and `yamux`.
3. Define a custom `NetworkBehaviour` struct (using the `#[derive(NetworkBehaviour)]` macro) that combines:
   - `gossipsub::Behaviour` (for broadcasting blocks and txs)
   - `kad::Behaviour` (for peer discovery)
   - `request_response::Behaviour` (for syncing blocks)

### Step 3: Create the Network Manager
1. Define a `NetworkManager` struct that holds the `libp2p::Swarm` and a channel to communicate with the central `Node`.
2. Implement a method `pub async fn start(&mut self)` that drives the `Swarm` by calling `swarm.select_next_some().await` in a loop.
3. Handle `SwarmEvent`s:
   - When a `GossipsubEvent::Message` is received, deserialize it into a `NetworkMessage` (Block or Transaction) and send it to the `Node`.
   - When a `RequestResponseEvent::Message` is received (a `GetBlocks` request), fetch the blocks from the `Node` and send them back.

### Step 4: Broadcast Messages
1. Add a method `pub async fn broadcast(&mut self, topic: &str, message: NetworkMessage)` to `NetworkManager` to publish a message to a Gossipsub topic.
2. Update the `Node` to notify the `NetworkManager` when a new transaction is added to the mempool or a new block is forged, so it can be published to the network.

### Step 5: Wire it all together
1. Update `src/main.rs` to initialize the `libp2p` Swarm and the `NetworkManager` alongside the `Node`.
2. Start the network listener (`swarm.listen_on(...)`) and dial any initial bootstrap peers provided via command-line arguments to join the DHT.

Good luck! This chapter will transform your single-node simulation into a real distributed system using production-grade networking tools.
