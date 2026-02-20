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

### Hints for `State`:
* Use a `HashMap` to store account balances. The key should be the `VerifyingKey` (as bytes, since `VerifyingKey` doesn't implement `Hash` directly, or you can use a wrapper/array). Actually, `[u8; 32]` is a great key for a `HashMap`.
* You'll need another `HashMap` for sequence numbers.
* **Borrow Checker:** When implementing `apply_tx`, you'll need to mutate the state. Pay attention to how you borrow `self` (`&mut self`) and how you access the `HashMap` entries. The `entry` API is very useful here: `self.balances.entry(key).or_insert(0)`.

### Hints for `Blockchain`:
* The `Blockchain` needs a `Vec<Block>` for the chain.
* It needs a `State` instance.
* It needs a `Vec<Transaction>` for the mempool.
* **Borrow Checker & Lifetimes:** When adding a block, you need to validate its transactions against the state. You might need to temporarily apply them to a clone of the state to see if they are valid together, or apply them and rollback if one fails. For this exercise, a simpler approach is to clone the state, apply the transactions to the clone, and if successful, replace the actual state with the clone. This avoids complex rollback logic but requires understanding how to clone and replace data.
* **Error Handling:** The `add_transaction` and `add_block` methods return `Result<(), &'static str>`. This is a simple way to handle errors in Rust. If validation fails, return `Err("Reason")`. If it succeeds, return `Ok(())`.

Go to `src/state.rs` and `src/blockchain.rs` and complete the `TODO`s. Run `cargo test` to verify your implementation.
