#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockError {
    InvalidSignature,
    InvalidHeight,
    InvalidSlot,
    InvalidValidator,
    InvalidTransaction,
    InvalidSlashProof,
    InvalidStateRoot,
}
