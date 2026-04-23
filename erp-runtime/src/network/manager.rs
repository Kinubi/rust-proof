use futures::{ SinkExt, TryFutureExt };
use rp_core::{ models::block::Block, traits::ToBytes };
use rp_node::{ blockchain, contract::PeerId, network::message::NetworkMessage };

use log::{ info, warn };

use crate::{ network::errors::NetworkError, runtime::node::{ EventTx, NetworkRx, RuntimeEvent } };

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

        let _ = self.event_tx
            .send(RuntimeEvent::FrameReceived {
                peer: self.peer,
                frame: vec![7],
            }).await
            .map_err(NetworkError::NetworkChannelSendError);
        Ok(())
    }
}
