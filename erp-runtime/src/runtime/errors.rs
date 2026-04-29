use std::io::Error;

use futures::channel::mpsc::SendError;
use rp_node::errors::{ ContractError, NodeError };
use esp_idf_hal::sys::EspError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    Event,
    Network,
    Storage,
    Wake,
}

#[derive(Debug)]
pub enum RuntimeError {
    Node(NodeError),
    Contract(ContractError),
    Crypto(&'static str),
    Config(&'static str),
    Esp(EspError),
    StorageInit(EspError),
    ChannelSend {
        channel: ChannelKind,
        source: SendError,
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

    pub fn esp(source: EspError) -> Self {
        Self::Esp(source)
    }

    pub fn event_send(source: SendError) -> Self {
        Self::ChannelSend {
            channel: ChannelKind::Event,
            source,
        }
    }

    pub fn network_send(source: SendError) -> Self {
        Self::ChannelSend {
            channel: ChannelKind::Network,
            source,
        }
    }

    pub fn storage_send(source: SendError) -> Self {
        Self::ChannelSend {
            channel: ChannelKind::Storage,
            source,
        }
    }

    pub fn wake_send(source: SendError) -> Self {
        Self::ChannelSend {
            channel: ChannelKind::Wake,
            source,
        }
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

impl From<EspError> for RuntimeError {
    fn from(value: EspError) -> Self {
        Self::Esp(value)
    }
}
