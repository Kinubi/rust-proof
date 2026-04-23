use futures::channel::mpsc::SendError;
use rp_node::errors::ContractError;

#[derive(Debug)]
pub enum StorageError {
    StorageChannelSendError(SendError),
    ContractError(ContractError),
}
