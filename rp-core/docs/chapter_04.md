# Chapter 4: Proof of Stake Consensus

Right now, our blockchain is a bit too trusting. The `Blockchain::add_block` method accepts a block from *anyone* as long as the cryptographic signature is valid. 

In a real blockchain, we need a **Consensus Mechanism** to decide *who* is allowed to forge the next block. If anyone can forge a block at any time, the network will be flooded with competing blocks, and we won't have a single source of truth.

We are going to implement a simplified **Proof of Stake (PoS)** system.

## Domain Knowledge: Proof of Stake

In Proof of Work (Bitcoin), miners compete to solve a math puzzle. The first to solve it gets to forge the block. This requires massive amounts of electricity.

In Proof of Stake, validators "stake" (lock up) their own coins. The protocol then deterministically selects a validator to forge the next block based on how much stake they have. 
*   **More stake = higher chance of being selected.**
*   **Slashing:** If a validator acts maliciously (e.g., tries to forge two different blocks at the same height), their staked coins are destroyed (slashed). This provides economic security.

### Our Simplified PoS Model

To keep things manageable, we will implement a deterministic, round-robin style selection based on active stakes.

1.  **Staking Transactions:** We need a new type of transaction where a user locks up their coins to become a validator.
2.  **The Validator Set:** The `State` needs to keep track of who is currently a validator and how much they have staked.
3.  **The Selection Algorithm:** Given the current block height and the validator set, we need a function that deterministically returns the `VerifyingKey` of the validator who is allowed to forge the *next* block.
4.  **Block Validation:** When `add_block` is called, we must check if the `block.validator` is actually the one chosen by our selection algorithm for that specific block height.

## The Rust Focus: Enums, Pattern Matching, and Iterators

This chapter will heavily test your ability to use Rust's `enum`s to represent different types of data, and how to use `match` statements to handle them safely. We will also use Iterator methods to calculate total stakes and select validators.

## Your Task: Implementing Consensus

We need to refactor our `Transaction` model and update our `State` and `Blockchain`.

### Step 1: Refactoring `Transaction`

Currently, `Transaction` only represents a transfer of funds. We need it to also represent staking.

1.  Open `src/models/transaction.rs`.
2.  Create a new `enum` called `TransactionData`:
    ```rust
    #[derive(Debug, Clone)]
    pub enum TransactionData {
        Transfer { receiver: VerifyingKey, amount: u64 },
        Stake { amount: u64 },
    }
    ```
3.  Modify the `Transaction` struct. Remove `receiver` and `amount`, and replace them with `pub data: TransactionData`.
4.  **Fix the fallout:** You will need to update `ToBytes` for `Transaction` to handle the new enum. (Hint: write a byte `0` for Transfer and `1` for Stake, followed by their respective fields). You will also need to fix the tests in `transaction.rs`.

### Step 2: Updating `State`

1.  Open `src/state.rs`.
2.  Add a new field to the `State` struct: `pub stakes: HashMap<[u8; 32], u64>`.
3.  Update `State::new()` to initialize it.
4.  Update `is_valid_tx`:
    *   If it's a `Transfer`, check if `balance >= amount`.
    *   If it's a `Stake`, check if `balance >= amount`.
5.  Update `apply_tx`:
    *   If it's a `Transfer`, subtract from sender, add to receiver.
    *   If it's a `Stake`, subtract from sender's balance, and *add* to their entry in the `stakes` HashMap.

### Step 3: The Selection Algorithm

1.  In `src/state.rs`, add a new method to `State`:
    ```rust
    pub fn get_expected_validator(&self, next_block_height: u64) -> Option<VerifyingKey>
    ```
2.  **The Algorithm:**
    *   If `self.stakes` is empty, return `None`.
    *   Calculate the `total_stake` across all validators.
    *   Use the `next_block_height` as a seed to pick a winner. A simple way: `let winning_ticket = next_block_height % total_stake;`
    *   Iterate through the `stakes` HashMap. Keep a running sum of the stakes. When the running sum exceeds the `winning_ticket`, that validator is the winner!
    *   *Note: Iterating over a HashMap is non-deterministic in Rust. To make consensus work, you must sort the keys first before iterating!*

### Step 4: Enforcing Consensus

1.  Open `src/blockchain.rs`.
2.  In `add_block`, before applying transactions, add a new check:
    *   Call `self.state.get_expected_validator(block.height)`.
    *   If it returns `Some(expected_key)`, ensure that `block.validator == expected_key`. If not, return `Err("Invalid validator for this block height")`.
    *   *(If it returns `None`, it means there are no validators yet. For testing purposes, you might want to allow anyone to forge if the stake pool is empty, or require a hardcoded genesis validator).*

Go ahead and start refactoring `Transaction`! This is a big architectural change, so take it step by step and rely on the compiler errors to guide you.