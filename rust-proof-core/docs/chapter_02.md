# Chapter 2: State Management and The Blockchain

Now that we have our core data structures (`Transaction` and `Block`) and the cryptographic primitives to secure them, we need to build the actual blockchain and manage the state of the network.

## Domain Knowledge: State and The Ledger

A blockchain is essentially a state machine. The "State" is the current balance of every account on the network.
* **Genesis State:** The initial state of the network, usually defining some initial balances.
* **State Transitions:** Transactions are instructions to change the state (e.g., "subtract 10 from Alice, add 10 to Bob").
* **Blocks:** Blocks batch these state transitions together. When a block is added to the chain, the state is updated.

### The Mempool
When users create transactions, they don't go directly into a block. They are broadcast to the network and sit in a "Memory Pool" (Mempool) waiting to be picked up by a validator and included in the next block.

### Validation Rules
Before a transaction can be added to the mempool or a block, it must be validated:
1. **Cryptographic Validity:** Is the signature valid? (We did this in Chapter 1).
2. **State Validity:** Does the sender have enough balance to cover the transaction amount?
3. **Sequence Validity:** Is the sequence number correct? (To prevent replay attacks, each transaction from an account must have a strictly increasing sequence number).

## Your Task: The `State` and `Blockchain` Structs

We will create two new modules: `state.rs` and `blockchain.rs`.

1. Open `src/state.rs`.
2. Implement the `State` struct. It needs to keep track of balances and the next expected sequence number for each account.
3. Implement the logic to apply a transaction to the state.
4. Open `src/blockchain.rs`.
5. Implement the `Blockchain` struct. It will hold the chain of blocks, the current state, and the mempool.
6. Implement the logic to add a new block to the chain, validating it against the current state.

### Hints for `State` (in `src/state.rs`):
* **`State` struct:** Use a `HashMap<[u8; 32], u64>` for `balances` and another for `nonces`. The key is the byte representation of the `VerifyingKey`.
* **`get_balance` & `get_nonce`:** Convert the `account` to bytes (`account.to_bytes()`) and use `.get(&bytes).copied().unwrap_or(0)` to return the value or 0 if it doesn't exist.
* **`is_valid_tx`:** 
  1. Check `tx.is_valid()` (cryptographic signature).
  2. Check if `self.get_balance(&tx.sender) >= tx.amount`.
  3. Check if `self.get_nonce(&tx.sender) == tx.sequence`.
* **`apply_tx`:** 
  1. Subtract `tx.amount` from the sender's balance: `*self.balances.entry(sender_bytes).or_insert(0) -= tx.amount;`
  2. Add `tx.amount` to the receiver's balance: `*self.balances.entry(receiver_bytes).or_insert(0) += tx.amount;`
  3. Increment the sender's nonce: `*self.nonces.entry(sender_bytes).or_insert(0) += 1;`

### Hints for `Blockchain` (in `src/blockchain.rs`):
* **`Blockchain` struct:** Needs `chain: Vec<Block>`, `state: State`, and `mempool: Vec<Transaction>`.
* **`new`:** Create a genesis block (height 0, empty previous hash, empty transactions, dummy validator key). Initialize `State` and `mempool`. Return the `Blockchain` with the genesis block in the chain.
* **`add_transaction`:** Call `self.state.is_valid_tx(&tx)`. If true, push to `self.mempool` and return `Ok(())`. Else, return `Err("Invalid transaction")`.
* **`add_block`:** 
  1. Check `block.is_valid()`.
  2. Check `block.height == self.get_latest_block().height + 1`.
  3. Check `block.previous_hash == self.get_latest_block().hash()`.
  4. Create a temporary state clone: `let mut temp_state = self.state.clone();`
  5. Iterate over `block.transactions`. If `!temp_state.is_valid_tx(tx)`, return `Err("Invalid transaction in block")`. Otherwise, `temp_state.apply_tx(tx)`.
  6. If all valid, replace state (`self.state = temp_state`), push block to chain, and clear mempool (`self.mempool.clear()`). Return `Ok(())`.

Go to `src/state.rs` and `src/blockchain.rs` and complete the `TODO`s. Run `cargo test` to verify your implementation.
