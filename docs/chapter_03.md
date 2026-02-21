# Chapter 3: Concurrency, Channels, and the Node Architecture

We have a working blockchain state machine! We can create transactions, validate them, bundle them into blocks, and update our state. However, right now, it's just a single-threaded data structure sitting in memory.

A real blockchain node needs to do many things at once:
1. Listen for incoming network connections (other nodes).
2. Listen for user commands (e.g., via a CLI or RPC).
3. Mine/Forge new blocks if it's a validator.
4. Maintain the blockchain state.

If we try to do all of this in a single thread, the node will block. For example, while it's busy hashing a block, it won't be able to receive new transactions from the network.

## Domain Knowledge: The Node Architecture

To solve this, we will split our node into several concurrent **Tasks** (or threads).

1. **The State Manager (The Brain):** This task owns the `Blockchain` struct. It is the *only* task allowed to mutate the state. This prevents race conditions where two threads try to update an account balance at the same time.
2. **The Miner/Validator (The Worker):** This task constantly tries to create new blocks from the mempool.
3. **The Network/API Layer (The Ears):** This task listens for incoming data (transactions, blocks) from the outside world.

### How do they communicate? Message Passing!
Since only the State Manager owns the `Blockchain`, how does the Network layer add a new transaction to the mempool?
We use **Channels** (specifically, Multi-Producer, Single-Consumer or MPSC channels).

*   **Channels:** Think of a channel as a pipe. You can send messages into one end (the Transmitter/Producer) and read them out the other end (the Receiver/Consumer).
*   **MPSC:** Many tasks can have a clone of the Transmitter (Network, Miner, CLI), but only one task holds the Receiver (The State Manager).

When the Network layer receives a transaction, it sends a `Command::AddTransaction(tx)` message through the channel. The State Manager receives this message, validates the transaction, and updates its internal `Blockchain`.

## Your Task: The `Node` and Message Passing

We will create a new module: `node.rs`.

1. Open `src/node.rs`.
2. Define an `enum` called `NodeCommand`. This will represent all the possible messages that can be sent to the State Manager.
    *   `AddTransaction(Transaction)`
    *   `AddBlock(Block)`
    *   `GetLatestBlock(oneshot::Sender<Block>)` -> *Wait, what is `oneshot`?*
3. **The Return Trip (`oneshot`):** Sometimes, a task needs an answer back. If the CLI asks for the latest block, it sends a command, but it needs the block returned to it. We use a `oneshot` channel for this. It's a channel that can only send *one* message. The CLI creates a `oneshot` channel, sends the `Sender` half inside the `NodeCommand`, and waits on the `Receiver` half. The State Manager gets the command, gets the block, and sends it back through the `oneshot::Sender`.
4. Implement the `Node` struct. It should hold the `Receiver` for the `NodeCommand`s and the `Blockchain` itself.
5. Implement a `run` method on the `Node` that loops forever, receiving commands and acting on them.

### Hints for `NodeCommand` and `Node` (in `src/node.rs`):
*   **`NodeCommand` enum:** Define variants for `AddTransaction`, `AddBlock`, `GetLatestBlock`, and `GetMempool`. Each variant needs to carry the necessary data and a `oneshot::Sender` to return the result. For example: `AddTransaction { tx: Transaction, responder: oneshot::Sender<Result<(), &'static str>> }`.
*   **`Node` struct:** Needs `blockchain: Blockchain` and `receiver: mpsc::Receiver<NodeCommand>`.
*   **`Node::new`:** 
    1. Create an MPSC channel: `let (tx, rx) = mpsc::channel(100);`
    2. Initialize a new `Blockchain`.
    3. Return `(Node { blockchain, receiver: rx }, tx)`.
*   **`Node::run`:** 
    1. Use `while let Some(cmd) = self.receiver.recv().await { ... }` to process incoming commands.
    2. Match on the `cmd` enum variants.
    3. For `AddTransaction`, call `self.blockchain.add_transaction(tx)` and send the result back via `responder.send(...)`.
    4. For `AddBlock`, call `self.blockchain.add_block(block)` and send the result back.
    5. For `GetLatestBlock`, get the latest block (you might need to clone it) and send it back.
    6. For `GetMempool`, clone the mempool and send it back.
    *Note: `responder.send(...)` returns a Result. You can usually ignore the error if the receiver dropped the channel (e.g., `let _ = responder.send(...);`).*

Go to `Cargo.toml` and add `tokio`. Then create `src/node.rs` and complete the `TODO`s. Run `cargo check` to verify your implementation.
