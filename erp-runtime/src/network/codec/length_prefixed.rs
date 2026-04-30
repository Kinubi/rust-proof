use crate::runtime::errors::RuntimeError;

pub fn encode_length_prefixed(payload: &[u8], max_len: u32) -> Result<Vec<u8>, RuntimeError> {
    let _ = (payload, max_len);
    todo!("implement length-prefixed encoding")
}

pub fn decode_length_prefix(input: &[u8], max_len: u32) -> Result<(usize, &[u8]), RuntimeError> {
    let _ = (input, max_len);
    todo!("implement length-prefix decoding")
}
