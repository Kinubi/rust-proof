use rp_core::models::block::Block;
use rp_core::models::transaction::Transaction;
use alloc::vec::Vec;
use crate::errors::ContractError;

#[derive(Debug, Clone)]
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
        state_bytes: Option<Vec<u8>>,
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

#[derive(Debug, Clone)]
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
    CancelWake,
    DisconnectPeer {
        peer: PeerId,
    },
    ReportEvent {
        message: &'static str,
    },
    FrameReceived {
        peer: PeerId,
    },
    PersistCompleted {
        persist_type: PersistCompletedType,
    },
    LoadSnapshot {
        block_hash: BlockHash,
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
    fn send_frame(&mut self, peer: PeerId, frame: Frame) -> Result<(), ContractError>;
    fn broadcast_frame(&mut self, frame: Frame) -> Result<(), ContractError>;
    fn disconnect_peer(&mut self, peer: PeerId) -> Result<(), ContractError>;
    fn request_blocks(
        &mut self,
        peer: PeerId,
        from_height: u64,
        to_height: u64
    ) -> Result<(), ContractError>;
}

pub trait Storage {
    fn save_block(&mut self, block: &Block) -> Result<(), ContractError>;
    fn load_block(&mut self, hash: &BlockHash) -> Result<Option<Block>, ContractError>;

    fn save_snapshot(
        &mut self,
        block_hash: &BlockHash,
        state_bytes: &[u8]
    ) -> Result<(), ContractError>;
    fn load_snapshot(&mut self, block_hash: &BlockHash) -> Result<Option<Vec<u8>>, ContractError>;
}

pub trait Clock {
    fn now_ms(&self) -> u64;
}

pub trait Wake {
    fn schedule_wake(&mut self, at: WakeAt) -> Result<(), ContractError>;
    fn cancel_wake(&mut self) -> Result<(), ContractError>;
}

pub trait Identity {
    fn peer_id(&self) -> PeerId;
    fn public_key(&self) -> Vec<u8>;
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, ContractError>;
}
