use core::marker::PhantomData;

use serde::{ Serialize, de::DeserializeOwned };

use crate::runtime::errors::RuntimeError;

pub trait ValueCodec<T> {
    fn encode(item: &T) -> Result<Vec<u8>, RuntimeError>;
    fn decode(bytes: &[u8]) -> Result<T, RuntimeError>;
}

pub struct PostcardCodec<T>(PhantomData<T>);

impl<T> ValueCodec<T> for PostcardCodec<T> where T: Serialize + DeserializeOwned {
    fn encode(item: &T) -> Result<Vec<u8>, RuntimeError> {
        let _ = item;
        todo!("implement postcard encoding")
    }

    fn decode(bytes: &[u8]) -> Result<T, RuntimeError> {
        let _ = bytes;
        todo!("implement postcard decoding")
    }
}
