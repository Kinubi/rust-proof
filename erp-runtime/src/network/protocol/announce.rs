use rp_node::network::message::{AnnounceRequest, AnnounceResponse};

use crate::{
    network::codec::{
        length_prefixed::{decode_length_prefix, encode_length_prefixed},
        postcard_codec::{PostcardCodec, ValueCodec},
    },
    runtime::errors::RuntimeError,
};

pub const ANNOUNCE_PROTOCOL: &str = "/rust-proof/announce/1";

pub fn encode_announce_request(
    req: &AnnounceRequest,
    max_len: u32,
) -> Result<Vec<u8>, RuntimeError> {
    let payload = PostcardCodec::<AnnounceRequest>::encode(req)?;
    encode_length_prefixed(&payload, max_len)
}

pub fn decode_announce_request(
    frame: &[u8],
    max_len: u32,
) -> Result<AnnounceRequest, RuntimeError> {
    let (_, payload) = decode_length_prefix(frame, max_len)?;
    PostcardCodec::<AnnounceRequest>::decode(payload)
}

pub fn encode_announce_response(
    resp: &AnnounceResponse,
    max_len: u32,
) -> Result<Vec<u8>, RuntimeError> {
    let payload = PostcardCodec::<AnnounceResponse>::encode(resp)?;
    encode_length_prefixed(&payload, max_len)
}

pub fn decode_announce_response(
    frame: &[u8],
    max_len: u32,
) -> Result<AnnounceResponse, RuntimeError> {
    let (_, payload) = decode_length_prefix(frame, max_len)?;
    PostcardCodec::<AnnounceResponse>::decode(payload)
}
