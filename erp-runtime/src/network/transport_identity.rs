use rp_node::contract::Identity;

use crate::runtime::errors::RuntimeError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

pub struct TransportIdentityRecord {
    pub algorithm: TransportIdentityAlgorithm,
    pub secret_key: [u8; 32],
}

pub struct TransportIdentityManager;

impl TransportIdentityManager {
    pub fn load_or_create() -> Result<Self, RuntimeError> {
        todo!("implement transport identity loading and persistence")
    }

    pub fn peer_id_bytes(&self) -> &[u8] {
        todo!("implement transport peer id access")
    }

    pub fn public_key_protobuf_bytes(&self) -> &[u8] {
        todo!("implement transport public key access")
    }

    pub fn sign(&self, message: &[u8]) -> Result<Vec<u8>, RuntimeError> {
        let _ = message;
        todo!("implement transport identity signing")
    }
}

impl TransportIdentity for TransportIdentityManager {
    fn algorithm(&self) -> TransportIdentityAlgorithm {
        todo!("implement transport identity algorithm selection")
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

impl Identity for TransportIdentityManager {
    fn peer_id(&self) -> rp_node::contract::PeerId {
        todo!("implement transport peer id projection")
    }

    fn public_key(&self) -> Vec<u8> {
        TransportIdentity::public_key_protobuf_bytes(self)
    }

    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, rp_node::errors::ContractError> {
        let _ = message;
        todo!("implement transport identity contract signing bridge")
    }
}