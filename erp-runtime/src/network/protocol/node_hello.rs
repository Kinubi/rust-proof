use serde::{ Deserialize, Serialize };
use rp_core::{ crypto::{ Signature, Verifier, verifying_key_from_bytes }, traits::Hashable };
use rp_node::contract::Identity;

use crate::{ network::{ transport_identity::TransportIdentity }, runtime::errors::RuntimeError };

pub const NODE_HELLO_PROTOCOL: &str = "/rust-proof/node-hello/1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHello {
    pub version: u16,
    pub node_public_key: Vec<u8>,
    pub node_peer_id: [u8; 32],
    pub transport_peer_id: Vec<u8>,
    pub max_frame_len: u32,
    pub max_blocks_per_chunk: u16,
    pub capabilities: PeerCapabilities,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHelloResponse {
    pub accepted: bool,
    pub remote: NodeHello,
    pub reject_reason: Option<NodeHelloRejectReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeHelloRejectReason {
    VersionMismatch,
    InvalidSignature,
    PeerIdMismatch,
    TransportBindingMismatch,
    UnsupportedRequiredProtocol,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerCapabilities {
    pub supports_sync_v1: bool,
    pub supports_announce_v1: bool,
    pub supports_ping: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeHelloTranscript<'a> {
    pub version: u16,
    pub node_peer_id: [u8; 32],
    pub transport_peer_id: &'a [u8],
    pub max_frame_len: u32,
    pub max_blocks_per_chunk: u16,
    pub capabilities: &'a PeerCapabilities,
}

#[derive(Debug, Clone)]
pub struct VerifiedPeer {
    pub node_peer_id: [u8; 32],
    pub node_public_key: Vec<u8>,
    pub transport_peer_id: Vec<u8>,
    pub max_frame_len: u32,
    pub max_blocks_per_chunk: u16,
    pub capabilities: PeerCapabilities,
}

pub struct NodeHelloBuilder<'a> {
    pub node_identity: &'a dyn Identity,
    pub transport_identity: &'a dyn TransportIdentity,
    pub max_frame_len: u32,
    pub max_blocks_per_chunk: u16,
    pub capabilities: PeerCapabilities,
}

impl<'a> NodeHelloBuilder<'a> {
    pub fn build(&self) -> Result<NodeHello, RuntimeError> {
        let node_public_key = self.node_identity.public_key();
        let node_peer_id = self.node_identity.peer_id();
        let transport_peer_id = self.transport_identity.transport_peer_id();
        let transcript = NodeHelloTranscript {
            version: 1,
            node_peer_id,
            transport_peer_id: &transport_peer_id,
            max_frame_len: self.max_frame_len,
            max_blocks_per_chunk: self.max_blocks_per_chunk,
            capabilities: &self.capabilities,
        };
        let transcript_bytes = encode_transcript(&transcript)?;
        let signature = self.node_identity.sign(&transcript_bytes)?;

        Ok(NodeHello {
            version: transcript.version,
            node_public_key,
            node_peer_id,
            transport_peer_id,
            max_frame_len: self.max_frame_len,
            max_blocks_per_chunk: self.max_blocks_per_chunk,
            capabilities: self.capabilities.clone(),
            signature,
        })
    }
}

pub struct NodeHelloVerifier;

impl NodeHelloVerifier {
    pub fn verify(
        remote: &NodeHello,
        authenticated_transport_peer: &[u8]
    ) -> Result<VerifiedPeer, RuntimeError> {
        if remote.transport_peer_id.as_slice() != authenticated_transport_peer {
            return Err(
                RuntimeError::config(
                    "node hello transport peer id does not match authenticated session peer"
                )
            );
        }

        let derived_node_peer_id = remote.node_public_key.hash();
        if derived_node_peer_id != remote.node_peer_id {
            return Err(RuntimeError::config("node hello peer id does not match node public key"));
        }

        let verifying_key_bytes = remote.node_public_key
            .as_slice()
            .try_into()
            .map_err(|_|
                RuntimeError::config("node hello public key must be a compressed 33-byte P-256 key")
            )?;
        let verifying_key = verifying_key_from_bytes(&verifying_key_bytes).map_err(
            RuntimeError::crypto
        )?;

        let transcript = NodeHelloTranscript {
            version: remote.version,
            node_peer_id: remote.node_peer_id,
            transport_peer_id: &remote.transport_peer_id,
            max_frame_len: remote.max_frame_len,
            max_blocks_per_chunk: remote.max_blocks_per_chunk,
            capabilities: &remote.capabilities,
        };
        let transcript_bytes = encode_transcript(&transcript)?;
        let signature = Signature::from_slice(&remote.signature).map_err(|_|
            RuntimeError::crypto("invalid node hello signature bytes")
        )?;

        verifying_key
            .verify(&transcript_bytes, &signature)
            .map_err(|_| RuntimeError::config("node hello signature verification failed"))?;

        Ok(VerifiedPeer {
            node_peer_id: remote.node_peer_id,
            node_public_key: remote.node_public_key.clone(),
            transport_peer_id: remote.transport_peer_id.clone(),
            max_frame_len: remote.max_frame_len,
            max_blocks_per_chunk: remote.max_blocks_per_chunk,
            capabilities: remote.capabilities.clone(),
        })
    }
}

fn encode_transcript(transcript: &NodeHelloTranscript<'_>) -> Result<Vec<u8>, RuntimeError> {
    postcard
        ::to_allocvec(transcript)
        .map_err(|_| RuntimeError::crypto("failed to serialize node hello transcript"))
}
