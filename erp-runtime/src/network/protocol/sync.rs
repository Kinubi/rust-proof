use rp_node::network::message::{SyncRequest, SyncResponse};

use crate::{
    network::codec::{
        length_prefixed::{decode_length_prefix, encode_length_prefixed},
        postcard_codec::{PostcardCodec, ValueCodec},
    },
    runtime::errors::RuntimeError,
};

pub const SYNC_PROTOCOL: &str = "/rust-proof/sync/1";

pub fn encode_sync_request(req: &SyncRequest, max_len: u32) -> Result<Vec<u8>, RuntimeError> {
    let payload = PostcardCodec::<SyncRequest>::encode(req)?;
    encode_length_prefixed(&payload, max_len)
}

pub fn decode_sync_request(frame: &[u8], max_len: u32) -> Result<SyncRequest, RuntimeError> {
    let (_, payload) = decode_length_prefix(frame, max_len)?;
    PostcardCodec::<SyncRequest>::decode(payload)
}

pub fn encode_sync_response(resp: &SyncResponse, max_len: u32) -> Result<Vec<u8>, RuntimeError> {
    let payload = PostcardCodec::<SyncResponse>::encode(resp)?;
    encode_length_prefixed(&payload, max_len)
}

pub fn decode_sync_response(frame: &[u8], max_len: u32) -> Result<SyncResponse, RuntimeError> {
    let (_, payload) = decode_length_prefix(frame, max_len)?;
    PostcardCodec::<SyncResponse>::decode(payload)
}
