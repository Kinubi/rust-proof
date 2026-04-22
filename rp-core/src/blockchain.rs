use crate::models::block::Block;
use crate::state::State;

pub struct AppliedBlock {
    pub next_state: State,
}

pub fn validate_and_apply_block(
    parent_block: &Block,
    parent_state: &State,
    block: &Block
) -> Result<AppliedBlock, String> {
    if !block.is_valid() {
        return Err("Invalid block signature".to_string());
    }

    if block.height != parent_block.height + 1 {
        return Err("Invalid block height".to_string());
    }
    if block.slot <= parent_block.slot {
        return Err("Block slot must be greater than the parent block's slot".to_string());
    }

    let mut next_state = parent_state.clone();
    if let Some(expected_validator) = next_state.get_expected_validator(block.height) {
        if block.validator != expected_validator {
            return Err("Invalid block validator".to_string());
        }
    }

    for tx in &block.transactions {
        if !next_state.is_valid_tx(tx) {
            return Err("Invalid transaction in block".to_string());
        }
        next_state.apply_tx(tx, block.slot);
    }

    for proof in &block.slash_proofs {
        if let Err(e) = next_state.apply_slash(proof.clone()) {
            return Err(format!("Invalid slash proof: {}", e));
        }
    }

    let computed_state_root = next_state.compute_state_root();
    if block.state_root != computed_state_root {
        return Err("Invalid state root".to_string());
    }

    Ok(AppliedBlock { next_state: next_state })
}

pub fn should_replace_head(
    candidate_hash: &[u8; 32],
    candidate_block: &Block,
    current_head: &Block,
    head_hash: [u8; 32]
) -> bool {
    if candidate_block.height != current_head.height {
        return candidate_block.height > current_head.height;
    }

    if candidate_block.slot != current_head.slot {
        return candidate_block.slot > current_head.slot;
    }

    *candidate_hash > head_hash
}
