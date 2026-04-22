#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockError {
    InvalidSignature,
    InvalidHeight,
    InvalidSlot,
    InvalidValidator,
    InvalidTransaction,
    InvalidSlashProof(SlashError),
    InvalidStateRoot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashError {
    DifferentSlots,
    IdenticalBlocks,
    ValidatorMismatchA,
    ValidatorMismatchB,
    InvalidBlockA,
    InvalidBlockB,
}
