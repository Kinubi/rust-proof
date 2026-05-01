use std::io::{Error, ErrorKind};

use unsigned_varint::{decode, encode};

use crate::runtime::errors::RuntimeError;

pub fn encode_length_prefixed(payload: &[u8], max_len: u32) -> Result<Vec<u8>, RuntimeError> {
    if payload.len() > (max_len as usize) {
        return Err(RuntimeError::NetworkError(Error::new(
            ErrorKind::InvalidData,
            "payload exceeds configured frame length limit",
        )));
    }

    let mut buffer = encode::u32_buffer();
    let prefix = encode::u32(payload.len() as u32, &mut buffer);
    let mut framed = Vec::with_capacity(prefix.len() + payload.len());
    framed.extend_from_slice(prefix);
    framed.extend_from_slice(payload);
    Ok(framed)
}

pub fn decode_length_prefix(input: &[u8], max_len: u32) -> Result<(usize, &[u8]), RuntimeError> {
    let (payload_len, remaining) = decode::u32(input).map_err(|error| {
        RuntimeError::NetworkError(Error::new(
            ErrorKind::InvalidData,
            format!("invalid length prefix: {error}"),
        ))
    })?;

    if payload_len > max_len {
        return Err(RuntimeError::NetworkError(Error::new(
            ErrorKind::InvalidData,
            "length prefix exceeds configured frame length limit",
        )));
    }

    let header_len = input.len() - remaining.len();
    let total_len = header_len + (payload_len as usize);
    if input.len() < total_len {
        return Err(RuntimeError::NetworkError(Error::new(
            ErrorKind::UnexpectedEof,
            "frame body shorter than advertised length",
        )));
    }

    Ok((header_len, &input[header_len..total_len]))
}
