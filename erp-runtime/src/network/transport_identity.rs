use embedded_svc::storage::RawStorage;
use esp_idf_svc::nvs::{ EspKeyValueStorage, EspNvs, EspNvsPartition, NvsDefault };
use libp2p_identity::{ KeyType, Keypair, PublicKey, ecdsa };
use log::info;
use rp_core::crypto::Signature as P256Signature;
use rp_node::contract::Identity as _;
use serde::{ Deserialize, Serialize };

use crate::identity::manager::IdentityManager;
use crate::runtime::errors::RuntimeError;

const TAG: &str = "transport_identity";
const NVS_NAMESPACE: &str = "rp_net";
const TRANSPORT_IDENTITY_KEY: &str = "transport_id";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportIdentityAlgorithm {
    Ed25519,
    EcdsaP256,
}

pub trait TransportIdentity {
    fn algorithm(&self) -> TransportIdentityAlgorithm;
    fn transport_peer_id(&self) -> Vec<u8>;
    fn public_key_protobuf_bytes(&self) -> Vec<u8>;
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, RuntimeError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportIdentityRecord {
    pub algorithm: TransportIdentityAlgorithm,
    pub private_key_protobuf: Vec<u8>,
}

enum TransportIdentityBackend {
    Software(Keypair),
    Hardware(IdentityManager),
}

pub struct TransportIdentityManager {
    algorithm: TransportIdentityAlgorithm,
    peer_id_bytes: Vec<u8>,
    public_key_protobuf_bytes: Vec<u8>,
    backend: TransportIdentityBackend,
}

impl TransportIdentityManager {
    pub fn load_or_create() -> Result<Self, RuntimeError> {
        if let Some(identity) = IdentityManager::try_efuse()? {
            info!(target: TAG, "using efuse-backed runtime identity for libp2p transport authentication");
            return Self::from_hardware_identity(identity);
        }

        let mut storage = Self::open_storage()?;

        if let Some(record) = Self::load_record(&storage)? {
            return Self::from_record(record);
        }

        let keypair = Keypair::generate_ecdsa();
        let record = TransportIdentityRecord {
            algorithm: TransportIdentityAlgorithm::EcdsaP256,
            private_key_protobuf: keypair
                .to_protobuf_encoding()
                .map_err(|_| RuntimeError::crypto("failed to encode transport identity keypair"))?,
        };

        Self::store_record(&mut storage, &record)?;
        info!(target: TAG, "generated and persisted libp2p transport identity in NVS");

        Self::from_record(record)
    }

    pub fn peer_id_bytes(&self) -> &[u8] {
        &self.peer_id_bytes
    }

    pub fn public_key_protobuf_bytes(&self) -> &[u8] {
        &self.public_key_protobuf_bytes
    }

    pub fn sign(&self, message: &[u8]) -> Result<Vec<u8>, RuntimeError> {
        match &self.backend {
            TransportIdentityBackend::Software(keypair) =>
                keypair
                    .sign(message)
                    .map_err(|_|
                        RuntimeError::crypto("failed to sign message with transport identity")
                    ),
            TransportIdentityBackend::Hardware(identity) =>
                identity
                    .sign(message)
                    .map_err(|_|
                        RuntimeError::crypto(
                            "failed to sign message with hardware transport identity"
                        )
                    )
                    .and_then(|raw_signature| {
                        P256Signature::from_slice(&raw_signature)
                            .map(|signature| signature.to_der().as_bytes().to_vec())
                            .map_err(|_|
                                RuntimeError::crypto(
                                    "hardware transport signature was not a valid P-256 signature"
                                )
                            )
                    }),
        }
    }

    fn open_storage() -> Result<EspKeyValueStorage<NvsDefault>, RuntimeError> {
        let partition = EspNvsPartition::<NvsDefault>::take()?;
        let nvs = EspNvs::new(partition, NVS_NAMESPACE, true)?;
        Ok(EspKeyValueStorage::new(nvs))
    }

    fn load_record(
        storage: &EspKeyValueStorage<NvsDefault>
    ) -> Result<Option<TransportIdentityRecord>, RuntimeError> {
        let Some(len) = RawStorage::len(storage, TRANSPORT_IDENTITY_KEY)? else {
            return Ok(None);
        };

        let mut buffer = vec![0u8; len];
        let Some(bytes) = storage.get_raw(TRANSPORT_IDENTITY_KEY, &mut buffer)? else {
            return Ok(None);
        };

        let record = postcard
            ::from_bytes(bytes)
            .map_err(|_| RuntimeError::crypto("invalid persisted transport identity record"))?;

        Ok(Some(record))
    }

    fn store_record(
        storage: &mut EspKeyValueStorage<NvsDefault>,
        record: &TransportIdentityRecord
    ) -> Result<(), RuntimeError> {
        let bytes = postcard
            ::to_allocvec(record)
            .map_err(|_| RuntimeError::crypto("failed to serialize transport identity record"))?;

        storage.set_raw(TRANSPORT_IDENTITY_KEY, &bytes)?;
        Ok(())
    }

    fn from_record(record: TransportIdentityRecord) -> Result<Self, RuntimeError> {
        let keypair = Keypair::from_protobuf_encoding(&record.private_key_protobuf).map_err(|_|
            RuntimeError::crypto("failed to decode persisted transport identity keypair")
        )?;

        let algorithm = Self::algorithm_from_key_type(keypair.key_type())?;
        if algorithm != record.algorithm {
            return Err(
                RuntimeError::crypto(
                    "transport identity algorithm does not match persisted keypair"
                )
            );
        }

        Self::from_keypair(keypair)
    }

    fn from_keypair(keypair: Keypair) -> Result<Self, RuntimeError> {
        let algorithm = Self::algorithm_from_key_type(keypair.key_type())?;
        let public_key = keypair.public();
        let public_key_protobuf_bytes = public_key.encode_protobuf();
        let peer_id_bytes = public_key.to_peer_id().to_bytes();

        Ok(Self {
            algorithm,
            peer_id_bytes,
            public_key_protobuf_bytes,
            backend: TransportIdentityBackend::Software(keypair),
        })
    }

    fn from_hardware_identity(identity: IdentityManager) -> Result<Self, RuntimeError> {
        let raw_public_key = identity.public_key();
        let ecdsa_public_key = ecdsa::PublicKey
            ::try_from_bytes(&raw_public_key)
            .map_err(|_|
                RuntimeError::crypto(
                    "failed to convert hardware public key into libp2p ECDSA identity"
                )
            )?;
        let public_key = PublicKey::from(ecdsa_public_key);
        let public_key_protobuf_bytes = public_key.encode_protobuf();
        let peer_id_bytes = public_key.to_peer_id().to_bytes();

        Ok(Self {
            algorithm: TransportIdentityAlgorithm::EcdsaP256,
            peer_id_bytes,
            public_key_protobuf_bytes,
            backend: TransportIdentityBackend::Hardware(identity),
        })
    }

    fn algorithm_from_key_type(
        key_type: KeyType
    ) -> Result<TransportIdentityAlgorithm, RuntimeError> {
        match key_type {
            KeyType::Ecdsa => Ok(TransportIdentityAlgorithm::EcdsaP256),
            KeyType::Ed25519 => Ok(TransportIdentityAlgorithm::Ed25519),
            _ => Err(RuntimeError::config("unsupported libp2p transport identity algorithm")),
        }
    }
}

impl TransportIdentity for TransportIdentityManager {
    fn algorithm(&self) -> TransportIdentityAlgorithm {
        self.algorithm
    }

    fn transport_peer_id(&self) -> Vec<u8> {
        self.peer_id_bytes().to_vec()
    }

    fn public_key_protobuf_bytes(&self) -> Vec<u8> {
        TransportIdentityManager::public_key_protobuf_bytes(self).to_vec()
    }

    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, RuntimeError> {
        TransportIdentityManager::sign(self, message)
    }
}
