use futures::AsyncWriteExt;
use serde::{ Serialize, de::DeserializeOwned };

use crate::{
    network::codec::{
        length_prefixed::encode_length_prefixed,
        postcard_codec::{ PostcardCodec, ValueCodec },
    },
    runtime::errors::RuntimeError,
};

pub async fn write_postcard_frame<S, T>(
    stream: &mut S,
    value: &T,
    max_len: u32
) -> Result<(), RuntimeError>
where
    S: futures::io::AsyncWrite + Unpin,
    T: Serialize + DeserializeOwned,
{
    let payload = PostcardCodec::<T>::encode(value)?;
    let frame = encode_length_prefixed(&payload, max_len)?;
    stream.write_all(&frame).await.map_err(RuntimeError::NetworkError)?;
    stream.flush().await.map_err(RuntimeError::NetworkError)
}

pub async fn read_postcard_frame<S, T>(stream: &mut S, max_len: u32) -> Result<T, RuntimeError>
where
    S: futures::io::AsyncRead + Unpin,
    T: Serialize + DeserializeOwned,
{
    rp_codec::postcard::read_postcard_frame(stream, max_len)
        .await
        .map_err(RuntimeError::NetworkError)
}

pub async fn read_length_prefixed_frame<S>(
    stream: &mut S,
    max_len: u32
) -> Result<Vec<u8>, RuntimeError>
where
    S: futures::io::AsyncRead + Unpin,
{
    rp_codec::length_prefixed::read_length_prefixed_frame(stream, max_len)
        .await
        .map_err(RuntimeError::NetworkError)
}