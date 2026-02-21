# Chapter 6: The Mempool and Fee Markets

Welcome to Chapter 6! In this chapter, we will upgrade our node's transaction handling. Currently, our `Blockchain` struct holds a simple `Vec<Transaction>` for the mempool. This is inefficient and lacks a crucial feature of real blockchains: **Fee Markets**.

When a network gets congested, users attach "fees" (or gas) to their transactions to incentivize validators to include them in the next block. Validators want to maximize their profit, so they should prioritize transactions with the highest fees.

## Domain Knowledge: The Mempool

The **Mempool** (Memory Pool) is a staging area for unconfirmed transactions. When a node receives a new transaction (either from a user or gossiped by a peer), it validates it and places it in the mempool.

A production-grade mempool needs to handle:
1.  **Prioritization:** Sorting transactions by fee per byte (or total fee).
2.  **Capacity Limits:** The mempool cannot grow infinitely. If it gets full, it must evict the lowest-paying transactions.
3.  **Staleness:** If a transaction sits in the mempool for too long, or if its sequence number (nonce) becomes invalid because another transaction from the same sender was confirmed, it must be removed.

## Rust Concepts: Custom Ordering and BTreeMaps

To efficiently sort transactions by fee, we need a data structure that maintains order. While we could use a `Vec` and call `.sort()` every time we add a transaction, this is $O(N \log N)$ and very slow for a large mempool.

Instead, we will use a `BTreeMap` (or a `BinaryHeap`). A `BTreeMap` keeps its keys sorted automatically. To use a custom struct as a key in a `BTreeMap`, we must implement the `Ord`, `PartialOrd`, `Eq`, and `PartialEq` traits.

## Step 1: Adding Fees to Transactions

First, we need to update our `Transaction` model to include a fee.

1.  Open `src/models/transaction.rs`.
2.  Add a `pub fee: u64` field to the `Transaction` struct.
3.  Update the `ToBytes` implementation for `Transaction` to include the `fee` bytes.
4.  Update any tests in `transaction.rs`, `state.rs`, `blockchain.rs`, and `node.rs` that create transactions to include a `fee` (you can just set it to `0` or `10` for existing tests).

## Step 2: Implementing Custom Ordering

We want to sort transactions primarily by their `fee` (highest first). If two transactions have the same fee, we need a tie-breaker to ensure deterministic ordering (e.g., sorting by their hash or sequence number).

1.  In `src/models/transaction.rs`, implement `PartialEq`, `Eq`, `PartialOrd`, and `Ord` for `Transaction`.
    *   *Hint:* In `Ord::cmp`, compare `other.fee.cmp(&self.fee)` (notice the reversed order to sort descending!). If fees are equal, compare their hashes.

## Step 3: Creating the Mempool Struct

Let's extract the mempool logic out of `Blockchain` and into its own dedicated struct.

1.  Create a new file: `src/mempool.rs`.
2.  Define a `Mempool` struct. It should contain:
    *   A collection to store the transactions (e.g., `std::collections::BTreeSet<Transaction>` or a `BTreeMap<[u8; 32], Transaction>` if you want to easily look up by hash).
    *   A `max_size: usize` to limit how many transactions we hold.
3.  Implement methods:
    *   `new(max_size: usize) -> Self`
    *   `add_transaction(&mut self, tx: Transaction) -> Result<(), &'static str>`: This should check if the mempool is full. If it is, it should only accept the new transaction if its fee is higher than the lowest fee currently in the mempool (and evict the lowest one).
    *   `get_pending_transactions(&self, count: usize) -> Vec<Transaction>`: Returns the top `count` transactions with the highest fees.
    *   `remove_transaction(&mut self, hash: &[u8; 32])`: Removes a specific transaction (useful when a block is mined).

## Step 4: Integrating Mempool into Blockchain

1.  Open `src/blockchain.rs`.
2.  Replace the `mempool: Vec<Transaction>` field with `mempool: crate::mempool::Mempool`.
3.  Update `Blockchain::new` to initialize the new `Mempool` (e.g., with a max size of 10,000).
4.  Update `add_transaction` to delegate to `self.mempool.add_transaction(tx)`.
5.  Update `add_block`. When a block is added, you must remove the transactions included in that block from the mempool.

## Step 5: Testing the Fee Market

Write tests in `src/mempool.rs` to verify:
1.  Transactions are returned in descending order of their fees.
2.  When the mempool hits `max_size`, adding a low-fee transaction is rejected.
3.  When the mempool hits `max_size`, adding a high-fee transaction succeeds and evicts the lowest-fee transaction.

Good luck! This chapter will heavily test your understanding of Rust's trait system (`Ord`, `Eq`) and standard library collections.