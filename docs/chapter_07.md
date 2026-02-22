# Chapter 7: Advanced Proof of Stake (Epochs & Slashing)

Welcome to Chapter 7! Our current Proof of Stake (PoS) implementation is very naive. It selects validators based on block height, allows immediate staking, and has no penalties for bad behavior. In a real blockchain, this is highly insecure. 

In this chapter, we will introduce **Time (Slots & Epochs)**, **Unbonding Periods**, and **Slashing**.

## Domain Knowledge

### 1. Slots and Epochs
Instead of relying purely on block height, modern PoS chains (like Ethereum or Solana) divide time into **Slots** and **Epochs**.
*   **Slot:** A fixed window of time (e.g., 12 seconds). Exactly one validator is chosen to propose a block during a slot. If they are offline, the slot is empty (a "skipped" slot), and the chain moves to the next slot.
*   **Epoch:** A collection of slots (e.g., 32 slots). The validator schedule and stake weights are usually fixed at the beginning of an epoch to ensure predictability.

### 2. The "Nothing at Stake" Problem & Slashing
In Proof of Work, mining on two different forks simultaneously splits your hash power (costing you money). In naive Proof of Stake, signing blocks on multiple forks costs nothing. A malicious validator could sign every possible fork to guarantee they end up on the winning chain. This is the **Nothing at Stake** problem.

To fix this, we introduce **Slashing**. If a validator signs two different blocks for the exact same `slot`, they have committed a mathematically provable crime (equivocation / double-signing). Anyone can submit a `SlashProof` containing the two conflicting signatures. The network will then "slash" (destroy) the malicious validator's staked funds and kick them out.

### 3. Unbonding Periods (Cooldowns)
Because validators can be slashed for past crimes, they cannot be allowed to instantly withdraw their stake. If they could, a validator could double-sign, instantly unstake, and escape the penalty. 
When a validator submits an `Unstake` transaction, their funds enter an **Unbonding Period** (e.g., they are locked for 2 epochs). During this time, they no longer earn rewards, but they can still be slashed if a past crime is discovered.

---

## Your Tasks

I have placed `TODO: Chapter 7` comments in the codebase to guide you. Here is the roadmap:

### Step 1: Add Time to Blocks
1. Open `src/models/block.rs`.
2. Add a `pub slot: u64` field to the `Block` struct.
3. Update the `ToBytes` implementation for `Block` to include the `slot` bytes.
4. Fix any compiler errors in your tests by adding a dummy `slot` value to block creations.

### Step 2: Implement Unstaking
1. Open `src/models/transaction.rs`.
2. Add an `Unstake { amount: u64 }` variant to the `TransactionData` enum.
3. Update the `ToBytes` implementation to handle the new `Unstake` variant (e.g., use `[2u8]` as the prefix).

### Step 3: State Management for Unstaking
1. Open `src/state.rs`.
2. Define a new struct `UnstakeRequest { pub amount: u64, pub unlock_slot: u64 }`.
3. Add a new field to `State`: `pub unstaking: HashMap<[u8; 32], UnstakeRequest>`.
4. Update `State::new()` to initialize it.
5. Update `is_valid_tx` to validate `Unstake` transactions (ensure the user has enough staked funds).
6. Update `apply_tx` to handle `Unstake`. It should subtract from `stakes` and add to `unstaking` with an `unlock_slot` set to the current slot + some unbonding delay (e.g., `current_slot + 100`). *(Note: You might need to pass the current slot into `apply_tx` now!)*

### Step 4: Slashing
1. I have created a new file `src/models/slashing.rs` with a basic `SlashProof` struct.
2. Implement the `is_valid()` method on `SlashProof`. It must verify that:
   - Both blocks have the **same slot**.
   - Both blocks have **different hashes**.
   - Both blocks are signed by the **same validator**.
   - Both signatures are valid.
3. In `src/state.rs`, add a method `pub fn apply_slash(&mut self, proof: &SlashProof) -> Result<(), &'static str>`. If the proof is valid, set the validator's stake and unstaking balances to `0`!
4. **Wire it up:** Add a way for the network to process `SlashProof`s. You can either:
   - Add a `SubmitSlashProof` command to `NodeCommand` and a corresponding method in `Blockchain`.
   - Add a `Slash { proof: SlashProof }` variant to `TransactionData` and handle it in `apply_tx`.
   - Add a `pub slash_proofs: Vec<SlashProof>` field to the `Block` struct and process them in `Blockchain::add_block`.

### Step 5: Update Consensus (Optional / Advanced)
Update `Blockchain::add_block` to enforce that the block's `slot` is strictly greater than the `latest_block.slot`. 

### Step 6: Fork Choice Rule (Advanced)
In a real network, you might receive a block that doesn't build on your `latest_block`, but instead builds on an older block (a fork). 
1. Update `Blockchain` to store a tree of blocks rather than a single `Vec<Block>`.
2. Implement a "Heaviest Chain" rule: when multiple valid chains exist, the node should consider the chain with the most total stake behind its blocks as the canonical chain.

Good luck! This chapter introduces complex state transitions and cryptographic proofs of misbehavior.