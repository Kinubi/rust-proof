use rp_core::errors::BlockError;

#[derive(Debug, PartialEq, Eq)]
pub enum NodeError {
    BlockError(BlockError),
    ParentBlockNotFoundError,
    ParentSnapshotNotFoundError,
    BlockStorageError,
    StateStorageError,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ContractError {
    Storage,
    Transport,
    Wake,
    Identity,
    Other,
}
