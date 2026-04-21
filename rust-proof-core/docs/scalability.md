# Scalability & Storage Optimization

As the `rust-proof` blockchain grows, the state trie and historical block data will become massive. To ensure the network remains decentralized and accessible to everyday users, we must implement scalability and storage optimizations.

## 1. State Pruning & Archival Nodes
Currently, the RocksDB state stores every historical state trie. This is unsustainable for long-term growth.
*   **Pruning:** Implement state pruning to discard historical state tries, keeping only the latest state (e.g., the last 128 blocks).
*   **Node Types:** Differentiate between "Full Nodes" (pruned, fast sync) and "Archive Nodes" (keep everything, useful for block explorers and analytics).
*   **Implementation:** We can use a reference-counted trie structure or a garbage collection mechanism to safely delete unreferenced nodes from the database.

## 2. State Rent / Expiry
The state trie is a shared resource. If users can create accounts and leave them dormant forever, the state size will grow unbounded.
*   **State Rent:** Charge accounts a continuous fee (rent) for taking up space in the state trie.
*   **Eviction:** If an account runs out of funds to pay rent, its state is temporarily evicted to cold storage.
*   **Restoration:** Users can restore their evicted accounts by providing a Merkle proof of their last known state and paying a restoration fee.

## 3. Sharding or Rollups
To significantly increase transaction throughput, we can lay the groundwork for Layer 2 scaling solutions.
*   **Sharding:** Divide the network into multiple parallel chains (shards) that process transactions independently, with a central beacon chain coordinating cross-shard communication.
*   **Rollups:** Add support for verifying ZK-Rollup proofs or Optimistic fraud proofs directly in the state transition logic. This allows off-chain sequencers to process thousands of transactions and submit a single cryptographic proof to the main chain.
*   **Data Availability:** Implement data availability sampling (DAS) to ensure rollup data is available without requiring every node to download it.
