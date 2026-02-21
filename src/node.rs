use crate::blockchain::Blockchain;
use crate::models::block::Block;
use crate::models::transaction::Transaction;
use tokio::sync::{ mpsc, oneshot };

/// Commands that can be sent to the Node to interact with the Blockchain state.
// ============================================================================
// TODO 1: Define the `NodeCommand` enum.
// It should have variants for:
// - `AddTransaction`: Needs to carry a `Transaction` and a `oneshot::Sender<Result<(), &'static str>>`.
// - `AddBlock`: Needs to carry a `Block` and a `oneshot::Sender<Result<(), &'static str>>`.
// - `GetLatestBlock`: Needs to carry a `oneshot::Sender<Block>`.
// - `GetMempool`: Needs to carry a `oneshot::Sender<Vec<Transaction>>`.
// ============================================================================
#[derive(Debug)]
pub enum NodeCommand {
    // YOUR CODE HERE
}

/// The Node is the central state manager. It owns the Blockchain and processes
/// commands sequentially to ensure thread-safe state updates.
// ============================================================================
// TODO 2: Define the `Node` struct.
// It needs to hold:
// - `blockchain`: The `Blockchain` instance.
// - `receiver`: The receiving half of an MPSC channel (`mpsc::Receiver<NodeCommand>`).
// ============================================================================
pub struct Node {
    // YOUR CODE HERE
}

impl Node {
    /// Creates a new Node and returns it along with the sender half of the command channel.
    pub fn new() -> (Self, mpsc::Sender<NodeCommand>) {
        // ====================================================================
        // TODO 3: Implement `Node::new()`.
        // 1. Create an MPSC channel with a reasonable capacity (e.g., 100).
        // 2. Initialize a new `Blockchain`.
        // 3. Return the `Node` instance and the `Sender` half of the channel.
        // ====================================================================
        unimplemented!("Implement Node::new")
    }

    /// The main loop of the Node. It continuously listens for commands and processes them.
    /// This function takes ownership of the Node and runs indefinitely.
    pub async fn run(mut self) {
        // ====================================================================
        // TODO 4: Implement the main event loop.
        // 1. Use `while let Some(cmd) = self.receiver.recv().await` to process incoming commands.
        // 2. Match on the `cmd` enum variants.
        // 3. For `AddTransaction`, call `self.blockchain.add_transaction(tx)` and send the result back via `responder.send(...)`.
        // 4. For `AddBlock`, call `self.blockchain.add_block(block)` and send the result back.
        // 5. For `GetLatestBlock`, get the latest block (you might need to clone it) and send it back.
        // 6. For `GetMempool`, clone the mempool and send it back.
        // Note: `responder.send(...)` returns a Result. You can usually ignore the error if the receiver dropped the channel (e.g., `let _ = responder.send(...);`).
        // ====================================================================
        unimplemented!("Implement the Node's main event loop")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{ SigningKey, Signer };
    use rand::rngs::OsRng;
    use crate::traits::Hashable;

    #[tokio::test]
    async fn test_node_commands() {
        let (node, sender) = Node::new();

        // Spawn the node in a background task
        tokio::spawn(async move {
            node.run().await;
        });

        // Test GetLatestBlock
        // let (resp_tx, resp_rx) = oneshot::channel();
        // sender.send(NodeCommand::GetLatestBlock { responder: resp_tx }).await.unwrap();
        // let latest_block = resp_rx.await.unwrap();
        // assert_eq!(latest_block.height, 0); // Genesis block

        // Test AddTransaction
        let mut csprng = OsRng;
        let sender_keypair = SigningKey::generate(&mut csprng);
        let receiver_keypair = SigningKey::generate(&mut csprng);

        let mut tx = Transaction {
            sender: sender_keypair.verifying_key(),
            receiver: receiver_keypair.verifying_key(),
            amount: 50,
            sequence: 0,
            signature: None,
        };
        let hash = tx.hash();
        tx.signature = Some(sender_keypair.sign(&hash[..]));

        // let (resp_tx, resp_rx) = oneshot::channel();
        // We use a dummy command here just to make the test compile while you implement the enum
        // sender.send(NodeCommand::AddTransaction { tx: tx.clone(), responder: resp_tx }).await.unwrap();

        // It should fail because the sender has no balance in the genesis state
        // let result = resp_rx.await.unwrap();
        // assert!(result.is_err());
    }
}
