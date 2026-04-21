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

Here is the roadmap:

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

### Step 6: Fork Choice Rule & State Reorganization (The State Root Route)
In a real network, you might receive a block that doesn't build on your `latest_block`, but instead builds on an older block (a fork). To handle this, we need a **Fork Choice Rule** (Heaviest Chain) and a way to manage **State Reorganizations**.

Because applying transactions mutates the `State`, switching to a different fork means we need the exact `State` as it existed at that fork's tip. Instead of recalculating from genesis, we will use the **State Root Route**: saving snapshots of the state.

#### 6.1: Update the Block Structure
1. Open `src/models/block.rs`.
2. Add `pub state_root: [u8; 32]` to the `Block` struct. This represents the hash of the state *after* this block's transactions are applied.
3. Update `ToBytes` to include `state_root`.
4. You already have `BlockNode`, which is perfect for building the tree:
   ```rust
   pub struct BlockNode {
       pub block: Block,
       pub children: Vec<[u8; 32]>, // Hashes of child blocks
   }
   ```

#### 6.2: Update Storage to Handle State Snapshots
1. Open `src/storage.rs`.
2. Add methods to your `Storage` trait to save and load full `State` snapshots:
   - `fn save_state_snapshot(&self, block_hash: &[u8; 32], state: &State) -> Result<(), String>;`
   - `fn get_state_snapshot(&self, block_hash: &[u8; 32]) -> Result<Option<State>, String>;`
3. Implement these in `SledStorage`. *(Hint: You will need to implement `ToBytes` / `FromBytes` for `State`, or use a serialization crate like `serde` or `bincode` to easily convert the `State` struct to bytes for Sled)*.

#### 6.3: Refactor Blockchain to use a Block Tree
1. Open `src/blockchain.rs`.
2. Change `chain: Vec<Block>` to `chain: HashMap<[u8; 32], BlockNode>`.
3. Add a `pub head_hash: [u8; 32]` field to track the current tip of the heaviest chain.
4. Update `Blockchain::new()` to insert the genesis block into the `HashMap`, save its initial state snapshot to storage, and set `head_hash`.

#### 6.4: The New `add_block` Algorithm
When a new block arrives, you can no longer assume it builds on `self.state`. It might build on an older block!
Update `add_block` to follow this logic:
1. **Find Parent:** Look up the block's `previous_hash` in the `chain` HashMap. If it doesn't exist, reject the block (or queue it as an orphan).
2. **Load Parent State:** Fetch the parent block's `State` snapshot from `Storage`.
3. **Apply & Verify:** Clone the parent state. Apply the new block's transactions to this cloned state.
4. **State Root Check:** Compute the new state root (`cloned_state.compute_state_root()`). Verify it matches the `state_root` declared in the block.
5. **Save:** Save the new block to the `chain` HashMap (and add its hash to its parent's `children` list). Save the `cloned_state` snapshot to `Storage` keyed by the new block's hash.
6. **Fork Choice (Heaviest Chain):** 
   - Calculate the "weight" of the new block's chain. (Weight = sum of the stakes of the validators who proposed blocks from the fork point up to this new block).
   - Calculate the "weight" of the current `head_hash` chain.
   - If the new chain is heavier, update `self.head_hash = new_block.hash()` and update `self.state = cloned_state`. This is a **State Reorg**!

Good luck! This chapter introduces complex state transitions and cryptographic proofs of misbehavior.