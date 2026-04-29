use futures::{ SinkExt, StreamExt };
use log::{ error, info };
use rp_core::traits::ToBytes;
use rp_node::{ contract::Identity, network::message::{ AnnounceRequest, NetworkMessage } };

use crate::{
    identity::manager::IdentityManager,
    runtime::block_signing::build_probe_block,
    runtime::errors::RuntimeError,
    runtime::manager::{ EventTx, NetworkCommand, NetworkRx, RuntimeEvent },
};

const TAG: &str = "manager";

pub struct NetworkManager {
    network_rx: NetworkRx,
    event_tx: EventTx,
    identity: IdentityManager,
}

impl NetworkManager {
    pub fn new(network_rx: NetworkRx, event_tx: EventTx, identity: IdentityManager) -> Self {
        Self { network_rx, event_tx, identity }
    }

    pub async fn run(&mut self) -> Result<(), RuntimeError> {
        info!(target: TAG, "Running Network");

        match build_probe_block(&self.identity) {
            Ok(probe_block) => {
                info!(
                    target: TAG,
                    "injecting probe block height={} slot={}",
                    probe_block.height,
                    probe_block.slot
                );

                self.event_tx
                    .send(RuntimeEvent::FrameReceived {
                        peer: self.identity.peer_id(),
                        frame: NetworkMessage::AnnounceRequest(
                            AnnounceRequest::block(probe_block)
                        ).to_bytes(),
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
