use std::{ fmt::Display, io::Error };

use tokio::sync::mpsc::error;
use rp_node::errors::{ ContractError, NodeError };

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    Event,
    Network,
    Storage,
    Wake,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelErrorKind {
    RuntimeEvent,
    NetworkCommand,
    StorageCommand,
    WakeCommand,
}

#[derive(Debug)]
pub enum RuntimeError {
    Node(NodeError),
    Contract(ContractError),
    Crypto(&'static str),
    Config(&'static str),
    ChannelSend {
        channel: ChannelKind,
        kind: ChannelErrorKind,
    },
    NetworkError(Error),
}

impl RuntimeError {
    pub fn crypto(message: &'static str) -> Self {
        Self::Crypto(message)
    }

    pub fn config(message: &'static str) -> Self {
        Self::Config(message)
    }

    pub fn event_send<T>(_source: error::SendError<T>) -> Self {
        Self::ChannelSend {
            channel: ChannelKind::Event,
            kind: ChannelErrorKind::RuntimeEvent,
        }
    }

    pub fn network_send<T>(_source: error::SendError<T>) -> Self {
        Self::ChannelSend {
            channel: ChannelKind::Network,
            kind: ChannelErrorKind::NetworkCommand,
        }
    }

    pub fn storage_send<T>(_source: error::SendError<T>) -> Self {
        Self::ChannelSend {
            channel: ChannelKind::Storage,
            kind: ChannelErrorKind::StorageCommand,
        }
    }

    pub fn wake_send<T>(_source: error::SendError<T>) -> Self {
        Self::ChannelSend {
            channel: ChannelKind::Wake,
            kind: ChannelErrorKind::WakeCommand,
        }
    }

    pub fn io_other(error: impl Display) -> Self {
        Self::NetworkError(Error::other(error.to_string()))
    }
}

impl From<NodeError> for RuntimeError {
    fn from(value: NodeError) -> Self {
        Self::Node(value)
    }
}

impl From<ContractError> for RuntimeError {
    fn from(value: ContractError) -> Self {
        Self::Contract(value)
    }
}
