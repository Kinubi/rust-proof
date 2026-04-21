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

This is a massive chapter. `libp2p` is powerful but complex. We will break it down into manageable pieces.

### Step 1: Add Dependencies
1. Open `Cargo.toml`.
2. Add the following dependencies:
   ```toml
   libp2p = { version = "0.53", features = ["tokio", "gossipsub", "kad", "tcp", "noise", "yamux", "macros", "request-response"] }
   serde = { version = "1.0", features = ["derive"] }
   bincode = "1.3"
   ```
3. You will also need to derive `Serialize` and `Deserialize` on your core models (`Block`, `Transaction`, `TransactionData`, `SlashProof`, `UnstakeRequest`).

### Step 2: Define Network Messages
1. Open `src/network/message.rs`.
2. Define an enum `NetworkMessage` that implements `Serialize` and `Deserialize`.
3. Add variants for:
   - `NewTransaction(crate::models::transaction::Transaction)`
   - `NewBlock(crate::models::block::Block)`
   - `SyncRequest { from_height: u64, to_height: u64 }`
   - `SyncResponse { blocks: Vec<crate::models::block::Block> }`

### Step 3: Create the libp2p Network Behavior
1. Open `src/network/manager.rs`.
2. Define a custom `NetworkBehaviour` struct using the `#[derive(NetworkBehaviour)]` macro. This struct combines the different protocols we want to use:
   ```rust
   #[derive(NetworkBehaviour)]
   pub struct AppBehaviour {
       pub gossipsub: gossipsub::Behaviour,
       pub kademlia: kad::Behaviour<kad::store::MemoryStore>,
       pub request_response: request_response::Behaviour<SyncCodec>,
   }
   ```
   *(Note: You will need to define a simple `SyncCodec` for the request/response protocol, or just use a basic byte-oriented codec provided by libp2p and serialize your `NetworkMessage` into it).*

### Step 4: Create the Network Manager
1. Define a `NetworkManager` struct that holds the `libp2p::Swarm<AppBehaviour>` and a channel (`mpsc::Sender<NodeCommand>`) to communicate with the central `Node`.
2. Implement a method `pub async fn start(&mut self)` that drives the `Swarm` by calling `self.swarm.select_next_some().await` in a loop.
3. Handle `SwarmEvent`s:
   - **`GossipsubEvent::Message`**: Deserialize the payload into a `NetworkMessage`. If it's a `NewBlock`, send a `NodeCommand::AddBlock` to the Node. If it's a `NewTransaction`, send a `NodeCommand::AddTransaction`.
   - **`RequestResponseEvent::Message::Request`**: A peer is asking for blocks. Send a command to the Node to fetch those blocks, then send them back via `self.swarm.behaviour_mut().request_response.send_response(...)`.
   - **`RequestResponseEvent::Message::Response`**: You received blocks you asked for. Send them to the Node to be processed.

### Step 5: Peer Discovery (Kademlia)
1. In your `NetworkManager::start` loop, handle `KademliaEvent`s.
2. When you discover a new peer (e.g., via `RoutingUpdated`), add them to Gossipsub: `self.swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id)`.
3. Periodically (e.g., every 5 minutes using `tokio::time::interval`), trigger a random Kademlia lookup to find new peers: `self.swarm.behaviour_mut().kademlia.get_closest_peers(PeerId::random())`.

### Step 6: Broadcast Messages
1. Add a method `pub async fn broadcast_transaction(&mut self, tx: Transaction)` to `NetworkManager`. Serialize the tx and publish it to a Gossipsub topic (e.g., `"transactions"`).
2. Add a method `pub async fn broadcast_block(&mut self, block: Block)`. Serialize the block and publish it to a Gossipsub topic (e.g., `"blocks"`).
3. Update the `Node` actor to notify the `NetworkManager` when a new transaction is added to the mempool or a new block is forged, so it can be published to the network.

### Step 7: Wire it all together
1. Update `src/main.rs` to initialize the `libp2p` Swarm. You will need to build a transport (TCP + Noise encryption + Yamux multiplexing).
2. Start the network listener (`swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)`).
3. If a bootstrap peer multiaddr is provided via command-line arguments, dial it (`swarm.dial(...)`) and add it to Kademlia to join the DHT.

Good luck! This chapter will transform your single-node simulation into a real distributed system using production-grade networking tools.
