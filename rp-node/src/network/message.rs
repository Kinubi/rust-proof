use rp_core::{ models::block::Block, traits::{ FromBytes, ToBytes } };
use rp_core::models::transaction::Transaction;
use serde::{ Deserialize, Serialize };
use alloc::vec::Vec;

#[derive(Serialize, Deserialize, Debug)]
pub enum NetworkMessage {
    NewTransaction(Transaction),
    NewBlock(Block),
    SyncRequest(SyncRequest),
    SyncResponse(SyncResponse),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SyncRequest {
    pub from_height: u64,
    pub to_height: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SyncResponse {
    pub blocks: Vec<Block>,
}

impl ToBytes for NetworkMessage {
    fn to_bytes(&self) -> Vec<u8> {
        postcard::to_allocvec(self).expect("network message serialization should succeed")
    }
}

impl ToBytes for SyncRequest {
    fn to_bytes(&self) -> Vec<u8> {
        postcard::to_allocvec(self).expect("sync request serialization should succeed")
    }
}

impl ToBytes for SyncResponse {
    fn to_bytes(&self) -> Vec<u8> {
        postcard::to_allocvec(self).expect("sync response serialization should succeed")
    }
}

impl FromBytes for NetworkMessage {
    fn from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        postcard::from_bytes(bytes).map_err(|_| "invalid network message")
    }
}

impl FromBytes for SyncRequest {
    fn from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        postcard::from_bytes(bytes).map_err(|_| "invalid sync request")
    }
}

impl FromBytes for SyncResponse {
    fn from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        postcard::from_bytes(bytes).map_err(|_| "invalid sync response")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;
    use rp_core::crypto::{ Signer, SigningKey };
    use rp_core::models::transaction::TransactionData;
    use rp_core::traits::Hashable;

    #[test]
    fn test_sync_request_round_trip() {
        let request = SyncRequest {
            from_height: 5,
            to_height: 10,
        };

        let encoded = request.to_bytes();
        let decoded = SyncRequest::from_bytes(&encoded).unwrap();

        assert_eq!(decoded.from_height, 5);
        assert_eq!(decoded.to_height, 10);
    }

    #[test]
    fn test_network_message_transaction_round_trip() {
        let mut csprng = OsRng;
        let sender = SigningKey::random(&mut csprng);
        let receiver = SigningKey::random(&mut csprng);

        let mut transaction = Transaction {
            sender: sender.verifying_key().clone(),
            data: TransactionData::Transfer {
                receiver: receiver.verifying_key().clone(),
                amount: 25,
            },
            sequence: 1,
            fee: 2,
            signature: None,
        };
        let hash = transaction.hash();
        transaction.signature = Some(sender.sign(&hash));

        let encoded = NetworkMessage::NewTransaction(transaction.clone()).to_bytes();
        let decoded = NetworkMessage::from_bytes(&encoded).unwrap();

        match decoded {
            NetworkMessage::NewTransaction(decoded_tx) => {
                assert_eq!(decoded_tx.sender, transaction.sender);
                assert_eq!(decoded_tx.sequence, transaction.sequence);
                assert_eq!(decoded_tx.fee, transaction.fee);
                assert_eq!(decoded_tx.signature, transaction.signature);
            }
            _ => panic!("expected transaction message"),
        }
    }
}
