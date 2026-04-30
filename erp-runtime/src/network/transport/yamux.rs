use core::marker::PhantomData;

use crate::runtime::errors::RuntimeError;

pub const YAMUX_PROTOCOL: &str = "/yamux/1.0.0";

pub struct YamuxMuxer<S> {
	_marker: PhantomData<S>,
}

pub struct YamuxSession<M> {
	pub muxer: M,
}

pub fn upgrade_outbound<S>(stream: S) -> Result<YamuxSession<YamuxMuxer<S>>, RuntimeError> {
	let _ = stream;
	todo!("implement outbound yamux upgrade")
}

pub fn upgrade_inbound<S>(stream: S) -> Result<YamuxSession<YamuxMuxer<S>>, RuntimeError> {
	let _ = stream;
	todo!("implement inbound yamux upgrade")
}
