use core::marker::PhantomData;

use serde::{Serialize, de::DeserializeOwned};

use crate::runtime::errors::RuntimeError;

pub trait ValueCodec<T> {
    fn encode(item: &T) -> Result<Vec<u8>, RuntimeError>;
    fn decode(bytes: &[u8]) -> Result<T, RuntimeError>;
}

pub struct PostcardCodec<T>(PhantomData<T>);

impl<T> ValueCodec<T> for PostcardCodec<T>
where
    T: Serialize + DeserializeOwned,
{
    fn encode(item: &T) -> Result<Vec<u8>, RuntimeError> {
        rp_codec::postcard::encode_postcard(item)
            .map_err(|_| RuntimeError::crypto("failed to serialize postcard payload"))
    }

    fn decode(bytes: &[u8]) -> Result<T, RuntimeError> {
        rp_codec::postcard::decode_postcard(bytes)
            .map_err(|_| RuntimeError::crypto("failed to deserialize postcard payload"))
    }
}
