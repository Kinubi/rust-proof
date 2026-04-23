use futures::StreamExt;
use rp_node::errors::NodeError;
use rp_node::node_engine::{ NodeEngine };
use rp_node::contract::{ NodeAction, NodeInput, PeerId };
use futures::channel::mpsc;
use log::{ info, warn };

const TAG: &str = "node";

pub enum RuntimeEvent {
    Tick {
        now_ms: u64,
    },
    PeerConnected {
        peer: PeerId,
    },
    PeerDisconnected {
        peer: PeerId,
    },
    FrameReceived {
        peer: PeerId,
        frame: Vec<u8>,
    },
    StorageLoaded {
        block_hash: [u8; 32],
        state_bytes: Option<Vec<u8>>,
    },
}

pub enum NetworkCommand {
    SendFrame {
        peer: PeerId,
        frame: Vec<u8>,
    },
    BroadcastFrame {
        frame: Vec<u8>,
    },
    DisconnectPeer {
        peer: PeerId,
    },
    RequestBlocks {
        peer: PeerId,
        from_height: u64,
        to_height: u64,
    },
}

pub type EventTx = mpsc::Sender<RuntimeEvent>;
pub type EventRx = mpsc::Receiver<RuntimeEvent>;
pub type NetworkTx = mpsc::Sender<NetworkCommand>;
pub type NetworkRx = mpsc::Receiver<NetworkCommand>;
pub struct NodeRuntime {
    node_engine: NodeEngine,
    event_rx: mpsc::Receiver<RuntimeEvent>,
    network_tx: mpsc::Sender<NetworkCommand>,
}
impl NodeRuntime {
    pub fn new(node_engine: NodeEngine, event_rx: EventRx, network_tx: NetworkTx) -> Self {
        Self { node_engine, event_rx, network_tx }
    }

    pub async fn run(&mut self) -> anyhow::Result<(), NodeError> {
        loop {
            let mut actions: Vec<NodeAction> = Vec::new();
            match self.event_rx.next().await.unwrap() {
                RuntimeEvent::Tick { now_ms } => {
                    actions = self.node_engine.step(NodeInput::Tick { now_ms });
                }
                RuntimeEvent::PeerConnected { peer } => {
                    actions = self.node_engine.step(NodeInput::PeerConnected { peer });
                }
                _ => {}
            }
            for action in actions {
                let _ = self.handle_action(action).await;
            }
        }
    }

    async fn handle_action(&mut self, action: NodeAction) -> anyhow::Result<()> {
        match action {
            NodeAction::ReportEvent { message } => {
                info!(target: TAG, "{message}");
                Ok(())
            }
            _ => { Ok(()) }
        }
    }
}
