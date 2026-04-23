use futures::{ SinkExt, StreamExt };
use log::info;
use rp_node::contract::PeerId;

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

        self.event_tx
            .send(RuntimeEvent::FrameReceived {
                peer: self.peer,
                frame: vec![7],
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
