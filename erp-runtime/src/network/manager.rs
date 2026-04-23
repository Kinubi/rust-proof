use futures::{ SinkExt, StreamExt };
use log::{ error, info };
use rp_core::traits::ToBytes;
use rp_node::{ contract::{ PeerId }, network::message::NetworkMessage };

use crate::{
    runtime::block_signing::build_probe_block,
    runtime::errors::RuntimeError,
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

    pub async fn run(&mut self) -> Result<(), RuntimeError> {
        info!(target: TAG, "Running Network");

        match build_probe_block() {
            Ok(probe_block) => {
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
                    .map_err(RuntimeError::event_send)?;
            }
            Err(error) => {
                error!(target: TAG, "probe block setup failed: {:?}", error);
            }
        }

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
