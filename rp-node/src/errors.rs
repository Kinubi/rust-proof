use rp_core::errors::BlockError;
use core::cmp::PartialEq;
#[derive(Debug)]
pub enum NodeError {
    BlockError(BlockError),
    ParentBlockNotFoundError,
    ParentSnapshotNotFoundError,
    BlockStorageError,
    StateStorageError,
}

impl PartialEq for NodeError {
    fn eq(&self, other: &Self) -> bool {
        self == other
    }
}
