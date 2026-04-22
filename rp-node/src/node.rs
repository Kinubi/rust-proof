use crate::blockchain::Blockchain;
use crate::storage::Storage;
use rp_core::models::block::Block;
use rp_core::models::slashing::SlashProof;
use rp_core::models::transaction::Transaction;
use tokio::sync::{ mpsc, oneshot };

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
    use ed25519_dalek::{ Signer, SigningKey };
    use rand::rngs::OsRng;
    use rp_core::models::transaction::{ Transaction, TransactionData };
    use rp_core::state::State;
    use rp_core::traits::Hashable;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct TestStorage {
        states: Mutex<HashMap<[u8; 32], State>>,
    }

    impl Storage for TestStorage {
        fn save_block(&self, _block: &Block) -> Result<(), String> {
            Ok(())
        }

        fn get_block(&self, _hash: &[u8; 32]) -> Result<Option<Vec<u8>>, String> {
            Ok(None)
        }

        fn save_state_root(&self, _height: u64, _root: &[u8; 32]) -> Result<(), String> {
            Ok(())
        }

        fn save_state_snapshot(&self, block_hash: &[u8; 32], state: &State) -> Result<(), String> {
            self.states.lock().unwrap().insert(*block_hash, state.clone());
            Ok(())
        }

        fn get_state_snapshot(&self, block_hash: &[u8; 32]) -> Result<Option<State>, String> {
            Ok(self.states.lock().unwrap().get(block_hash).cloned())
        }
    }

    #[tokio::test]
    async fn test_node_commands() {
        let storage = Box::new(TestStorage::default());
        let (node, sender) = Node::new(storage);

        tokio::spawn(async move {
            node.run().await;
        });

        let (resp_tx, resp_rx) = oneshot::channel();
        sender.send(NodeCommand::GetLatestBlock { responder: resp_tx }).await.unwrap();
        let latest_block = resp_rx.await.unwrap();
        assert_eq!(latest_block.height, 0);

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
        sender
            .send(NodeCommand::AddTransaction { transaction: tx.clone(), responder: resp_tx }).await
            .unwrap();

        let result = resp_rx.await.unwrap();
        assert!(result.is_err());
    }
}
