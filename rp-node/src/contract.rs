use rp_core::models::block::Block;
use rp_core::models::transaction::Transaction;

#[derive(Debug)]
pub enum PersistCompletedType {
    Snapshot,
    Block,
}

pub enum NodeInput {
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
        frame: Frame,
    },
    LocalTransactionSubmitted {
        transaction: Transaction,
    },
    StorageLoaded {
        block_hash: BlockHash,
        state_bytes: Vec<u8>,
    },
    PersistCompleted {
        persist_type: PersistCompletedType,
    },
    ImportRequested {
        peer: PeerId,
        from_height: u64,
        to_height: u64,
    },
}

pub enum NodeAction {
    SendFrame {
        peer: PeerId,
        frame: Frame,
    },
    BroadcastFrame {
        frame: Frame,
    },
    PersistBlock {
        block: Block,
    },
    PersistSnapshot {
        block_hash: BlockHash,
        state_bytes: Vec<u8>,
    },
    RequestBlocks {
        peer: PeerId,
        from_height: u64,
        to_height: u64,
    },
    ScheduleWake {
        at_ms: u64,
    },
    DisconnectPeer {
        peer: PeerId,
    },
    ReportEvent {
        message: String,
    },
}

pub type PeerId = [u8; 32];
pub type Frame = Vec<u8>;
pub type BlockHash = [u8; 32];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WakeAt {
    pub deadline_ms: u64,
}

pub trait Transport {
    type Error;

    fn send_frame(&mut self, peer: PeerId, frame: Frame) -> Result<(), Self::Error>;
    fn broadcast_frame(&mut self, frame: Frame) -> Result<(), Self::Error>;
    fn disconnect_peer(&mut self, peer: PeerId) -> Result<(), Self::Error>;
    fn request_blocks(
        &mut self,
        peer: PeerId,
        from_height: u64,
        to_height: u64
    ) -> Result<(), Self::Error>;
}

pub trait Storage {
    type Error;

    fn save_block(&mut self, block: &Block) -> Result<(), Self::Error>;
    fn load_block(&mut self, hash: &BlockHash) -> Result<Option<Block>, Self::Error>;

    fn save_snapshot(
        &mut self,
        block_hash: &BlockHash,
        state_bytes: &[u8]
    ) -> Result<(), Self::Error>;
    fn load_snapshot(&mut self, block_hash: &BlockHash) -> Result<Option<Vec<u8>>, Self::Error>;
}

pub trait Clock {
    fn now_ms(&self) -> u64;
}

pub trait Wake {
    type Error;

    fn schedule_wake(&mut self, at: WakeAt) -> Result<(), Self::Error>;
    fn cancel_wake(&mut self) -> Result<(), Self::Error>;
}

pub trait Identity {
    type Error;

    fn peer_id(&self) -> PeerId;
    fn public_key(&self) -> Vec<u8>;
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Self::Error>;
}
