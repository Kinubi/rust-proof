use crate::blockchain::Blockchain;
use crate::models::block::Block;
use crate::models::slashing::SlashProof;
use crate::models::transaction::{ Transaction };
use tokio::sync::{ mpsc, oneshot };
use crate::storage::{ Storage };

/// Commands that can be sent to the Node to interact with the Blockchain state.
#[derive(Debug)]
pub enum NodeCommand {
    AddTransaction {
        transaction: Transaction,
        responder: oneshot::Sender<Result<(), &'static str>>,
    },
    AddBlock {
        block: Block,
        responder: oneshot::Sender<Result<(), String>>,
    },
    GetLatestBlock {
        responder: oneshot::Sender<Block>,
    },
    GetMempool {
        responder: oneshot::Sender<Vec<Transaction>>,
    },
    SubmitSlashProof {
        proof: SlashProof,
        responder: oneshot::Sender<Result<(), &'static str>>,
    },
    GetBlocksByHeight {
        from_height: u64,
        to_height: u64,
        responder: oneshot::Sender<Result<Vec<Block>, String>>,
    },
}

/// The Node is the central state manager. It owns the Blockchain and processes
/// commands sequentially to ensure thread-safe state updates.
pub struct Node {
    blockchain: Blockchain,
    receiver: mpsc::Receiver<NodeCommand>,
}

impl Node {
    /// Creates a new Node and returns it along with the sender half of the command channel.
    pub fn new(storage: Box<dyn Storage>) -> (Self, mpsc::Sender<NodeCommand>) {
        let (sender, receiver) = mpsc::channel(100);
        let node = Node {
            blockchain: Blockchain::new(storage),
            receiver,
        };
        (node, sender)
    }

    /// The main loop of the Node. It continuously listens for commands and processes them.
    /// This function takes ownership of the Node and runs indefinitely.
    pub async fn run(mut self) {
        while let Some(cmd) = self.receiver.recv().await {
            match cmd {
                NodeCommand::AddTransaction { transaction, responder } => {
                    let result = self.blockchain.add_transaction(transaction);
                    let _ = responder.send(result);
                }
                NodeCommand::AddBlock { block, responder } => {
                    let result = self.blockchain.add_block(block);
                    let _ = responder.send(result);
                }
                NodeCommand::GetLatestBlock { responder } => {
                    let latest_block = self.blockchain.get_latest_block().clone();
                    let _ = responder.send(latest_block);
                }
                NodeCommand::GetMempool { responder } => {
                    let mempool = self.blockchain.get_mempool();
                    let _ = responder.send(mempool);
                }
                NodeCommand::SubmitSlashProof { proof, responder } => {
                    let result = self.blockchain.state.apply_slash(proof);
                    let _ = responder.send(result);
                }
                NodeCommand::GetBlocksByHeight { from_height, to_height, responder } => {
                    let result = self.blockchain.get_blocks(from_height, to_height);
                    let _ = responder.send(Ok(result));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{ SigningKey, Signer };
    use rand::rngs::OsRng;
    use crate::traits::Hashable;
    use crate::models::transaction::{ Transaction, TransactionData };
    use tokio::sync::{ oneshot };
    use crate::storage::{ SledStorage };

    #[tokio::test]
    async fn test_node_commands() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = Box::new(SledStorage::new(temp_dir.path().to_str().unwrap()).unwrap());
        let (node, sender) = Node::new(storage);

        // Spawn the node in a background task
        tokio::spawn(async move {
            node.run().await;
        });

        // Test GetLatestBlock
        let (resp_tx, resp_rx) = oneshot::channel();
        sender.send(NodeCommand::GetLatestBlock { responder: resp_tx }).await.unwrap();
        let latest_block = resp_rx.await.unwrap();
        assert_eq!(latest_block.height, 0); // Genesis block

        // Test AddTransaction
        let mut csprng = OsRng;
        let sender_keypair = SigningKey::generate(&mut csprng);
        let receiver_keypair = SigningKey::generate(&mut csprng);

        let mut tx = Transaction {
            sender: sender_keypair.verifying_key(),
            data: TransactionData::Transfer {
                receiver: receiver_keypair.verifying_key(),
                amount: 50,
            },
            sequence: 0,
            fee: 10,
            signature: None,
        };
        let hash = tx.hash();
        tx.signature = Some(sender_keypair.sign(&hash[..]));

        let (resp_tx, resp_rx) = oneshot::channel();
        // We use a dummy command here just to make the test compile while you implement the enum
        sender
            .send(NodeCommand::AddTransaction { transaction: tx.clone(), responder: resp_tx }).await
            .unwrap();

        // It should fail because the sender has no balance in the genesis state
        let result = resp_rx.await.unwrap();
        assert!(result.is_err());
    }
}
