use rp_node::network::message::{ SyncRequest, SyncResponse };

use crate::runtime::errors::RuntimeError;

pub const SYNC_PROTOCOL: &str = "/rust-proof/sync/1";

pub fn encode_sync_request(req: &SyncRequest, max_len: u32) -> Result<Vec<u8>, RuntimeError> {
	let _ = (req, max_len);
	todo!("implement sync request encoding")
}

pub fn decode_sync_request(frame: &[u8], max_len: u32) -> Result<SyncRequest, RuntimeError> {
	let _ = (frame, max_len);
	todo!("implement sync request decoding")
}

pub fn encode_sync_response(resp: &SyncResponse, max_len: u32) -> Result<Vec<u8>, RuntimeError> {
	let _ = (resp, max_len);
	todo!("implement sync response encoding")
}

pub fn decode_sync_response(frame: &[u8], max_len: u32) -> Result<SyncResponse, RuntimeError> {
	let _ = (frame, max_len);
	todo!("implement sync response decoding")
}
