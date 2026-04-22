use rp_core::errors::BlockError;
pub enum NodeError {
    BlockError(BlockError),
    BlockStorageError,
    StateStorageError,
}
