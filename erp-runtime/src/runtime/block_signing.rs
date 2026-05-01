use rp_core::{
    crypto::{Signature, VerifyingKey, verifying_key_from_bytes},
    models::{block::Block, slashing::SlashProof, transaction::Transaction},
    traits::Hashable,
};
use rp_node::{blockchain::Blockchain, contract::Identity, errors::ContractError};

use crate::runtime::errors::RuntimeError;

pub(crate) fn build_signed_block(
    parent: &Block,
    slot: u64,
    transactions: Vec<Transaction>,
    slash_proofs: Vec<SlashProof>,
    state_root: [u8; 32],
    identity: &impl Identity,
) -> Result<Block, RuntimeError> {
    let mut block = Block {
        height: parent.height + 1,
        slot,
        previous_hash: parent.hash(),
        validator: identity_verifying_key(identity)?,
        transactions,
        signature: None,
        slash_proofs,
        state_root,
    };
    let hash = block.hash();
    block.signature = Some(identity_signature(identity, &hash)?);
    Ok(block)
}

pub(crate) fn build_probe_block(identity: &impl Identity) -> Result<Block, RuntimeError> {
    let blockchain = Blockchain::new().expect("probe blockchain should initialize");
    let parent = blockchain.get_latest_block();

    build_signed_block(
        parent,
        parent.slot + 1,
        vec![],
        vec![],
        blockchain.state.compute_state_root(),
        identity,
    )
}

fn identity_verifying_key(identity: &impl Identity) -> Result<VerifyingKey, RuntimeError> {
    let public_key = identity.public_key();
    let key_bytes = public_key
        .as_slice()
        .try_into()
        .map_err(|_| ContractError::Identity)?;

    verifying_key_from_bytes(&key_bytes).map_err(|_| ContractError::Identity.into())
}

fn identity_signature(identity: &impl Identity, hash: &[u8]) -> Result<Signature, RuntimeError> {
    let signature_bytes = identity.sign(hash).map_err(RuntimeError::from)?;

    Signature::from_slice(&signature_bytes).map_err(|_| ContractError::Identity.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::identity::manager::IdentityManager;
    use rp_core::{crypto::signing_key_from_bytes, traits::ToBytes};
    use rp_node::contract::{NodeAction, NodeInput};
    use rp_node::node_engine::NodeEngine;

    #[test]
    fn test_signed_block_frame_is_accepted_by_node_engine() {
        let blockchain = Blockchain::new().unwrap();
        let parent = blockchain.get_latest_block().clone();
        let identity = IdentityManager::new(signing_key_from_bytes(&[7u8; 32]).unwrap());

        let block = build_signed_block(
            &parent,
            parent.slot + 1,
            vec![],
            vec![],
            blockchain.state.compute_state_root(),
            &identity,
        )
        .unwrap();

        let mut engine = NodeEngine::new(Blockchain::new().unwrap());
        let actions = engine.step(NodeInput::FrameReceived {
            peer: [0u8; 32],
            frame: rp_node::network::message::NetworkMessage::AnnounceRequest(
                rp_node::network::message::AnnounceRequest::block(block),
            )
            .to_bytes(),
        });

        assert_eq!(actions.len(), 2);
        assert!(matches!(actions[0], NodeAction::PersistBlock { .. }));
        assert!(matches!(actions[1], NodeAction::PersistSnapshot { .. }));
    }
}
