use std::net::SocketAddr;

use crate::{
	network::config::{ BootstrapPeer, MultiaddrLite, NetworkConfig },
	runtime::errors::RuntimeError,
};

pub fn resolve_bootstrap_addr(addr: &MultiaddrLite) -> Result<SocketAddr, RuntimeError> {
	let _ = addr;
	todo!("implement bootstrap address resolution")
}

pub fn bootstrap_targets(config: &NetworkConfig) -> &[BootstrapPeer] {
	let _ = config;
	todo!("implement bootstrap target selection")
}
