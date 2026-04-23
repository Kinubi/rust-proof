use core::fmt::Write;

use embedded_svc::storage::RawStorage;
use esp_idf_hal::sys::EspError;
use esp_idf_svc::nvs::{ EspKeyValueStorage, EspNvs, EspNvsPartition, NvsDefault };
use rp_core::{ models::block::Block, traits::Hashable };
use rp_node::{ contract::{ BlockHash, Storage }, errors::ContractError };
use std::string::String;
use log::info;

const TAG: &str = "nvs_storage";

pub struct NvsStorage {
    nvs_kv_storage: EspKeyValueStorage<NvsDefault>,
}

const NVS_NAMESPACE: &str = "rp";
const BLOCK_KEY_PREFIX: char = 'b';
const SNAPSHOT_KEY_PREFIX: char = 's';
const HASH_BUCKET_BYTES: usize = 6;

impl NvsStorage {
    pub fn new() -> Result<Self, EspError> {
        let nvs_partition = EspNvsPartition::<NvsDefault>::take()?;
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
        let Some(len) = RawStorage::len(&self.nvs_kv_storage, key).map_err(
            Self::map_storage_error
        )? else {
            return Ok(None);
        };

        let mut buffer = vec![0u8; len];
        let payload = self.nvs_kv_storage
            .get_raw(key, &mut buffer)
            .map_err(Self::map_storage_error)?;

        Ok(payload.map(|bytes| bytes.to_vec()))
    }

    fn write_blob(&mut self, key: &str, bytes: &[u8]) -> Result<(), ContractError> {
        self.nvs_kv_storage.set_raw(key, bytes).map_err(Self::map_storage_error)?;
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
        hash: &BlockHash
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
        payload: &[u8]
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
}

impl Storage for NvsStorage {
    fn save_block(&mut self, block: &Block) -> Result<(), ContractError> {
        let block_hash = block.hash();
        let block_bytes = postcard::to_allocvec(block).map_err(|_| ContractError::Storage)?;

        info!(target: TAG, "Block: {:?} saved", block.hash());
        self.save_hashed_blob(BLOCK_KEY_PREFIX, &block_hash, &block_bytes)
    }
    fn load_block(&mut self, hash: &BlockHash) -> Result<Option<Block>, ContractError> {
        let Some(block_bytes) = self.load_hashed_blob(BLOCK_KEY_PREFIX, hash)? else {
            return Ok(None);
        };

        let block: Block = postcard::from_bytes(&block_bytes).map_err(|_| ContractError::Storage)?;
        if block.hash() != *hash {
            return Err(ContractError::Storage);
        }

        Ok(Some(block))
    }

    fn save_snapshot(
        &mut self,
        block_hash: &BlockHash,
        state_bytes: &[u8]
    ) -> Result<(), ContractError> {
        self.save_hashed_blob(SNAPSHOT_KEY_PREFIX, block_hash, state_bytes)
    }
    fn load_snapshot(&mut self, block_hash: &BlockHash) -> Result<Option<Vec<u8>>, ContractError> {
        self.load_hashed_blob(SNAPSHOT_KEY_PREFIX, block_hash)
    }
}
