use crate::runtime::errors::RuntimeError;

pub const MULTISTREAM_V1: &str = "/multistream/1.0.0";

pub async fn write_protocol<S>(stream: &mut S, protocol: &str) -> Result<(), RuntimeError> {
	let _ = (stream, protocol);
	todo!("implement multistream protocol write")
}

pub async fn read_protocol<S>(stream: &mut S, max_len: usize) -> Result<String, RuntimeError> {
	let _ = (stream, max_len);
	todo!("implement multistream protocol read")
}

pub async fn dialer_select<S>(stream: &mut S, protocol: &str) -> Result<(), RuntimeError> {
	let _ = (stream, protocol);
	todo!("implement outbound multistream selection")
}

pub async fn listener_select<S>(stream: &mut S, supported: &[&str]) -> Result<String, RuntimeError> {
	let _ = (stream, supported);
	todo!("implement inbound multistream selection")
}
