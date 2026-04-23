use futures::channel::mpsc::SendError;

#[derive(Debug)]
pub enum NetworkError {
    NetworkError,
    NetworkChannelSendError(SendError),
}
