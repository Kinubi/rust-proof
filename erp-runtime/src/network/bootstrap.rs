use std::net::SocketAddr;
use std::net::ToSocketAddrs;

use crate::{
    network::config::{ BootstrapPeer, MultiaddrLite, NetworkConfig },
    runtime::errors::RuntimeError,
};

pub fn resolve_bootstrap_addr(addr: &MultiaddrLite) -> Result<SocketAddr, RuntimeError> {
    match addr {
        MultiaddrLite::Ip4Tcp { addr, port } => Ok(SocketAddr::from((*addr, *port))),
        MultiaddrLite::Dns4Tcp { host, port } =>
            (host.as_str(), *port)
                .to_socket_addrs()
                .map_err(RuntimeError::NetworkError)?
                .find(SocketAddr::is_ipv4)
                .ok_or_else(||
                    RuntimeError::config("bootstrap DNS name did not resolve to an IPv4 address")
                ),
    }
}

pub fn bootstrap_targets(config: &NetworkConfig) -> &[BootstrapPeer] {
    config.bootstrap_peers.as_slice()
}
