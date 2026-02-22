# Chapter 8: Networking & P2P Communication

Welcome to Chapter 8! Up until now, our blockchain has been running entirely on a single node. A blockchain isn't very useful if it can't communicate with other nodes in the network.

In this chapter, we will introduce **Networking** to allow multiple nodes to discover each other, share transactions, and broadcast new blocks.

## Domain Knowledge

### 1. Peer-to-Peer (P2P) Networks
Blockchains operate on a P2P network. There is no central server or master node. Every node connects to a few other nodes (peers) and relays information. When a node receives a new transaction or block, it validates it and then "gossips" it to its connected peers. This ensures that all nodes eventually reach the same state.

### 2. Network Protocols
To communicate, nodes need a shared language. We will define a simple protocol with the following message types:
*   **Handshake:** Sent when connecting to a new peer to exchange node information (e.g., current block height, genesis hash). This ensures both nodes are on the same network.
*   **Transaction:** Broadcast a new transaction to the network so it can be included in the mempool of other nodes.
*   **Block:** Broadcast a newly forged block to the network so other nodes can validate and append it to their chain.
*   **GetBlocks:** Request missing blocks from a peer to sync the chain if a node falls behind.

### 3. Asynchronous Networking
Networking is inherently asynchronous. Nodes can connect, disconnect, or send messages at any time. We will use `tokio` to handle asynchronous TCP connections and message parsing. `tokio` allows us to handle thousands of concurrent connections efficiently without blocking the main thread.

---

## Your Tasks

### Step 1: Define Network Messages
1. Create a new file `src/network/message.rs`.
2. Define an enum `NetworkMessage` that implements `Serialize` and `Deserialize` (you will need to add `serde` and `serde_json` or `bincode` to your `Cargo.toml`).
3. Add variants for:
   - `Handshake { height: u64, genesis_hash: [u8; 32] }`
   - `Transaction(crate::models::transaction::Transaction)`
   - `Block(crate::models::block::Block)`
   - `GetBlocks { from_height: u64, to_height: u64 }`

### Step 2: Create the Network Manager
1. Create a new file `src/network/manager.rs`.
2. Define a `NetworkManager` struct that holds a list of connected peers (e.g., `HashMap<SocketAddr, tokio::net::tcp::OwnedWriteHalf>`).
3. Implement a method `pub async fn start_listening(&mut self, port: u16)` to start listening for incoming connections using `tokio::net::TcpListener`.
4. Implement a method `pub async fn connect_to_peer(&mut self, addr: SocketAddr)` to connect to a known peer address using `tokio::net::TcpStream`.

### Step 3: Handle Incoming Messages
1. In `NetworkManager`, create a loop that reads incoming messages from connected peers using `tokio::io::AsyncReadExt`.
2. Deserialize the incoming bytes into a `NetworkMessage`.
3. When a `Transaction` message is received, send it to the `Node` via the `NodeCommand` channel.
4. When a `Block` message is received, send it to the `Node` via the `NodeCommand` channel.
5. When a `GetBlocks` message is received, fetch the requested blocks from the `Blockchain` and send them back to the peer.

### Step 4: Broadcast Messages
1. Add a method `pub async fn broadcast(&mut self, message: NetworkMessage)` to `NetworkManager` to send a message to all connected peers.
2. Update the `Node` to notify the `NetworkManager` when a new transaction is added to the mempool or a new block is forged, so it can be broadcasted. You might need to add a new channel from the `Node` to the `NetworkManager` for this.

### Step 5: Wire it all together
1. Update `src/main.rs` to initialize the `NetworkManager` alongside the `Node`.
2. Start the network listener and connect to any initial peers provided via command-line arguments (e.g., `cargo run -- --port 8000 --peer 127.0.0.1:8001`).

Good luck! This chapter will transform your single-node simulation into a real distributed system. Handling asynchronous networking and concurrent state updates is a significant milestone in building a blockchain.
