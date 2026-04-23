use futures::channel::mpsc::SendError;
use rp_node::errors::NodeError;

use crate::network::errors::NetworkError;

#[derive(Debug)]
pub enum RuntimeError {
    NodeError(NodeError),
    EventChannelSendError(SendError),
    NetworkChannelSendError(SendError),
    StorageChannelSendError(SendError),
    WakeChannelSendError(SendError),
    NetworkError(NetworkError),
}
