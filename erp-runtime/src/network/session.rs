use crate::{
    identity::manager::IdentityManager,
    network::{
        config::NetworkConfig,
        peer_registry::SessionId,
        transport_identity::TransportIdentityManager,
    },
    runtime::errors::RuntimeError,
};

pub struct SessionWorker<S> {
    pub session_id: SessionId,
    pub stream: S,
    pub role: ConnectionRole,
    pub node_identity: IdentityManager,
    pub transport_identity: TransportIdentityManager,
    pub config: NetworkConfig,
}

pub enum ConnectionRole {
    Inbound,
    Outbound,
}

impl<S> SessionWorker<S> {
    pub fn run(self) -> Result<(), RuntimeError> {
        todo!("implement session worker lifecycle")
    }
}
