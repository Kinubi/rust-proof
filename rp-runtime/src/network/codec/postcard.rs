use std::io;

use libp2p::futures::{ AsyncWrite, AsyncWriteExt };
use serde::{ Serialize, de::DeserializeOwned };

use crate::network::codec::length_prefixed::encode_length_prefixed;

pub use rp_codec::postcard::read_postcard_frame;

pub async fn write_postcard_frame<S, T>(stream: &mut S, value: &T, max_len: u32) -> io::Result<()>
    where S: AsyncWrite + Unpin + Send, T: Serialize + DeserializeOwned
{
    let payload = rp_codec::postcard::encode_postcard(value)?;
    let frame = encode_length_prefixed(&payload, max_len)?;
    stream.write_all(&frame).await?;
    stream.close().await
}
