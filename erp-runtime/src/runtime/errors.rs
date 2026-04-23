use futures::channel::mpsc::SendError;
use rp_node::errors::{ ContractError, NodeError };
use esp_idf_hal::sys::EspError;

use crate::{ network::errors::NetworkError, storage::errors::StorageError };

#[derive(Debug)]
pub enum RuntimeError {
    NodeError(NodeError),
    StorageInitError(EspError),
    StorageError(StorageError),
    EventChannelSendError(SendError),
    NetworkChannelSendError(SendError),
    StorageChannelSendError(SendError),
    WakeChannelSendError(SendError),
    NetworkError(NetworkError),
}
