use ed25519_dalek::{ Signer, SigningKey };
use futures::{ SinkExt, StreamExt };
use log::info;
use rp_core::{
    models::block::Block,
    traits::{ Hashable, ToBytes },
};
use rp_node::{ blockchain::Blockchain, contract::{ NodeInput, PeerId }, network::message::NetworkMessage, node_engine::NodeEngine };

use crate::{
    network::errors::NetworkError,
    runtime::node::{ EventTx, NetworkCommand, NetworkRx, RuntimeEvent },
};

const TAG: &str = "manager";

pub struct NetworkManager {
    network_rx: NetworkRx,
    event_tx: EventTx,
    peer: PeerId,
}

impl NetworkManager {
    pub fn new(network_rx: NetworkRx, event_tx: EventTx, peer: PeerId) -> Self {
        Self { network_rx, event_tx, peer }
    }

    pub async fn run(&mut self) -> anyhow::Result<(), NetworkError> {
        info!(target: TAG, "Running Network");

        let probe_block = build_probe_block();
        info!(
            target: TAG,
            "injecting probe block height={} slot={}",
            probe_block.height,
            probe_block.slot
        );

        self.event_tx
            .send(RuntimeEvent::FrameReceived {
                peer: self.peer,
                frame: NetworkMessage::NewBlock(probe_block).to_bytes(),
            }).await
            .map_err(NetworkError::NetworkChannelSendError)?;

        while let Some(command) = self.network_rx.next().await {
            match command {
                NetworkCommand::SendFrame { peer, .. } => {
                    info!(target: TAG, "send frame to peer: {:?}", peer);
                }
                NetworkCommand::BroadcastFrame { .. } => {
                    info!(target: TAG, "broadcast frame");
                }
                NetworkCommand::DisconnectPeer { peer } => {
                    info!(target: TAG, "disconnect peer: {:?}", peer);
                }
                NetworkCommand::RequestBlocks { peer, from_height, to_height } => {
                    info!(
                        target: TAG,
                        "request blocks from peer {:?}: {}..{}",
                        peer,
                        from_height,
                        to_height
                    );
                }
            }
        }

        Ok(())
    }
}

fn build_probe_block() -> Block {
    let blockchain = Blockchain::new().expect("probe blockchain should initialize");
    let parent = blockchain.get_latest_block().clone();
    let parent_state = blockchain.state.clone();
    let validator = SigningKey::from_bytes(&[7u8; 32]);

    let mut block = Block {
        height: parent.height + 1,
        slot: parent.slot + 1,
        previous_hash: parent.hash(),
        validator: validator.verifying_key(),
        transactions: vec![],
        signature: None,
        slash_proofs: vec![],
        state_root: parent_state.compute_state_root(),
    };
    let hash = block.hash();
    block.signature = Some(validator.sign(&hash));
    block
}

#[cfg(test)]
mod tests {
    use super::*;

    use rp_node::contract::NodeAction;

    #[test]
    fn test_probe_block_frame_is_accepted_by_node_engine() {
        let mut engine = NodeEngine::new(Blockchain::new().unwrap());
        let actions = engine.step(NodeInput::FrameReceived {
            peer: [0u8; 32],
            frame: NetworkMessage::NewBlock(build_probe_block()).to_bytes(),
        });

        assert_eq!(actions.len(), 2);
        assert!(matches!(actions[0], NodeAction::PersistBlock { .. }));
        assert!(matches!(actions[1], NodeAction::PersistSnapshot { .. }));
    }
}
