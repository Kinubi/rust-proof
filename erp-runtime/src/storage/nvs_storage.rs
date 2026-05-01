use core::fmt::Write;

use embedded_svc::storage::RawStorage;
use esp_idf_hal::sys::EspError;
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspKeyValueStorage, EspNvs, NvsDefault};
use log::warn;
use rp_core::{models::block::Block, traits::Hashable};
use rp_node::{
    contract::{BlockHash, Storage},
    errors::ContractError,
};
use std::string::String;

pub struct NvsStorage {
    nvs_kv_storage: EspKeyValueStorage<NvsDefault>,
}

const NVS_NAMESPACE: &str = "rp";
const BLOCK_KEY_PREFIX: char = 'b';
const SNAPSHOT_KEY_PREFIX: char = 's';
const HASH_BUCKET_BYTES: usize = 6;
const LATEST_SNAPSHOT_KEY: &str = "latest_snap";
const TAG: &str = "storage";

impl NvsStorage {
    pub fn new(nvs_partition: EspDefaultNvsPartition) -> Result<Self, EspError> {
        let nvs = EspNvs::new(nvs_partition, NVS_NAMESPACE, true)?;
        let nvs_kv_storage = EspKeyValueStorage::new(nvs);

        Ok(Self { nvs_kv_storage })
    }

    fn map_storage_error(_: EspError) -> ContractError {
        ContractError::Storage
    }

    fn slot_key(prefix: char, hash: &BlockHash, slot: u8) -> String {
        let mut key = String::with_capacity(15);
        key.push(prefix);

        for byte in hash.iter().take(HASH_BUCKET_BYTES) {
            write!(&mut key, "{byte:02x}").expect("NVS key formatting should succeed");
        }

        write!(&mut key, "{slot:02x}").expect("NVS key formatting should succeed");
        key
    }

    fn read_blob(&self, key: &str) -> Result<Option<Vec<u8>>, ContractError> {
        let Some(len) =
            RawStorage::len(&self.nvs_kv_storage, key).map_err(Self::map_storage_error)?
        else {
            return Ok(None);
        };

        let mut buffer = vec![0u8; len];
        let payload = self
            .nvs_kv_storage
            .get_raw(key, &mut buffer)
            .map_err(Self::map_storage_error)?;

        Ok(payload.map(|bytes| bytes.to_vec()))
    }

    fn write_blob(&mut self, key: &str, bytes: &[u8]) -> Result<(), ContractError> {
        self.nvs_kv_storage
            .set_raw(key, bytes)
            .map_err(Self::map_storage_error)?;
        Ok(())
    }

    fn encode_envelope(hash: &BlockHash, payload: &[u8]) -> Vec<u8> {
        let mut buffer = Vec::with_capacity(hash.len() + payload.len());
        buffer.extend_from_slice(hash);
        buffer.extend_from_slice(payload);
        buffer
    }

    fn load_hashed_blob(
        &self,
        prefix: char,
        hash: &BlockHash,
    ) -> Result<Option<Vec<u8>>, ContractError> {
        for slot in u8::MIN..=u8::MAX {
            let key = Self::slot_key(prefix, hash, slot);
            let Some(buffer) = self.read_blob(&key)? else {
                continue;
            };

            if buffer.len() < hash.len() {
                return Err(ContractError::Storage);
            }

            let (stored_hash, payload) = buffer.split_at(hash.len());
            if stored_hash == hash {
                return Ok(Some(payload.to_vec()));
            }
        }

        Ok(None)
    }

    fn save_hashed_blob(
        &mut self,
        prefix: char,
        hash: &BlockHash,
        payload: &[u8],
    ) -> Result<(), ContractError> {
        let encoded = Self::encode_envelope(hash, payload);

        for slot in u8::MIN..=u8::MAX {
            let key = Self::slot_key(prefix, hash, slot);

            match self.read_blob(&key)? {
                Some(buffer) => {
                    if buffer.len() < hash.len() {
                        return Err(ContractError::Storage);
                    }

                    let (stored_hash, _) = buffer.split_at(hash.len());
                    if stored_hash == hash {
                        return self.write_blob(&key, &encoded);
                    }
                }
                None => {
                    return self.write_blob(&key, &encoded);
                }
            }
        }

        Err(ContractError::Storage)
    }

    fn read_hash_key(&self, key: &str) -> Result<Option<BlockHash>, ContractError> {
        let Some(bytes) = self.read_blob(key)? else {
            return Ok(None);
        };

        if bytes.len() != BlockHash::default().len() {
            return Err(ContractError::Storage);
        }

        let mut hash = [0u8; 32];
        hash.copy_from_slice(&bytes);
        Ok(Some(hash))
    }

    fn write_hash_key(&mut self, key: &str, hash: &BlockHash) -> Result<(), ContractError> {
        self.write_blob(key, hash)
    }

    pub fn load_latest_snapshot_bundle(
        &mut self,
    ) -> Result<Option<(Block, Vec<u8>)>, ContractError> {
        let block_hash = match self.read_hash_key(LATEST_SNAPSHOT_KEY) {
            Ok(Some(block_hash)) => block_hash,
            Ok(None) => {
                return Ok(None);
            }
            Err(_) => {
                warn!(
                    target: TAG,
                    "ignoring invalid latest snapshot pointer; starting without a restored snapshot"
                );
                return Ok(None);
            }
        };

        let block = match self.load_block(&block_hash) {
            Ok(Some(block)) => block,
            Ok(None) => {
                warn!(
                    target: TAG,
                    "latest snapshot pointer references a missing block; starting without a restored snapshot"
                );
                return Ok(None);
            }
            Err(_) => {
                warn!(
                    target: TAG,
                    "latest snapshot pointer references an invalid block; starting without a restored snapshot"
                );
                return Ok(None);
            }
        };

        let state_bytes = match self.load_snapshot(&block_hash) {
            Ok(Some(state_bytes)) => state_bytes,
            Ok(None) => {
                warn!(
                    target: TAG,
                    "latest snapshot pointer references missing snapshot bytes; starting without a restored snapshot"
                );
                return Ok(None);
            }
            Err(_) => {
                warn!(
                    target: TAG,
                    "latest snapshot pointer references invalid snapshot bytes; starting without a restored snapshot"
                );
                return Ok(None);
            }
        };

        Ok(Some((block, state_bytes)))
    }
}

impl Storage for NvsStorage {
    fn save_block(&mut self, block: &Block) -> Result<(), ContractError> {
        let block_hash = block.hash();
        let block_bytes = postcard::to_allocvec(block).map_err(|_| ContractError::Storage)?;

        self.save_hashed_blob(BLOCK_KEY_PREFIX, &block_hash, &block_bytes)
    }
    fn load_block(&mut self, hash: &BlockHash) -> Result<Option<Block>, ContractError> {
        let Some(block_bytes) = self.load_hashed_blob(BLOCK_KEY_PREFIX, hash)? else {
            return Ok(None);
        };

        let block: Block =
            postcard::from_bytes(&block_bytes).map_err(|_| ContractError::Storage)?;
        if block.hash() != *hash {
            return Err(ContractError::Storage);
        }

        Ok(Some(block))
    }

    fn save_snapshot(
        &mut self,
        block_hash: &BlockHash,
        state_bytes: &[u8],
    ) -> Result<(), ContractError> {
        self.save_hashed_blob(SNAPSHOT_KEY_PREFIX, block_hash, state_bytes)?;
        self.write_hash_key(LATEST_SNAPSHOT_KEY, block_hash)
    }
    fn load_snapshot(&mut self, block_hash: &BlockHash) -> Result<Option<Vec<u8>>, ContractError> {
        self.load_hashed_blob(SNAPSHOT_KEY_PREFIX, block_hash)
    }
}
