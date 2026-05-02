use std::io::{self, ErrorKind};

use futures::io::AsyncRead;
use serde::{Serialize, de::DeserializeOwned};

use crate::length_prefixed::{decode_length_prefix, read_length_prefixed_frame};

pub fn encode_postcard<T>(value: &T) -> io::Result<Vec<u8>>
where
    T: Serialize + DeserializeOwned,
{
    postcard::to_allocvec(value)
        .map_err(|_| io::Error::new(ErrorKind::InvalidData, "failed to encode postcard payload"))
}

pub fn decode_postcard<T>(bytes: &[u8]) -> io::Result<T>
where
    T: Serialize + DeserializeOwned,
{
    postcard::from_bytes(bytes)
        .map_err(|_| io::Error::new(ErrorKind::InvalidData, "failed to decode postcard payload"))
}

pub async fn read_postcard_frame<S, T>(stream: &mut S, max_len: u32) -> io::Result<T>
where
    S: AsyncRead + Unpin,
    T: Serialize + DeserializeOwned,
{
    let frame = read_length_prefixed_frame(stream, max_len).await?;
    let (_, payload) = decode_length_prefix(&frame, max_len)?;
    decode_postcard(payload)
}