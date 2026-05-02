use crate::runtime::errors::RuntimeError;

pub fn encode_length_prefixed(payload: &[u8], max_len: u32) -> Result<Vec<u8>, RuntimeError> {
    rp_codec::length_prefixed
        ::encode_length_prefixed(payload, max_len)
        .map_err(RuntimeError::NetworkError)
}

pub fn decode_length_prefix(input: &[u8], max_len: u32) -> Result<(usize, &[u8]), RuntimeError> {
    rp_codec::length_prefixed
        ::decode_length_prefix(input, max_len)
        .map_err(RuntimeError::NetworkError)
}
