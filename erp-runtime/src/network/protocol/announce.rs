use rp_node::network::message::{ AnnounceRequest, AnnounceResponse };

use crate::runtime::errors::RuntimeError;

pub const ANNOUNCE_PROTOCOL: &str = "/rust-proof/announce/1";

pub fn encode_announce_request(
    req: &AnnounceRequest,
    max_len: u32
) -> Result<Vec<u8>, RuntimeError> {
    let _ = (req, max_len);
    todo!("implement announce request encoding")
}

pub fn decode_announce_request(
    frame: &[u8],
    max_len: u32
) -> Result<AnnounceRequest, RuntimeError> {
    let _ = (frame, max_len);
    todo!("implement announce request decoding")
}

pub fn encode_announce_response(
    resp: &AnnounceResponse,
    max_len: u32
) -> Result<Vec<u8>, RuntimeError> {
    let _ = (resp, max_len);
    todo!("implement announce response encoding")
}

pub fn decode_announce_response(
    frame: &[u8],
    max_len: u32
) -> Result<AnnounceResponse, RuntimeError> {
    let _ = (frame, max_len);
    todo!("implement announce response decoding")
}
