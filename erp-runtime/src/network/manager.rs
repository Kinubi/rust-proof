use esp_idf_hal::sys;
use futures::{ SinkExt, StreamExt };
use log::{ error, info, warn };
use rp_core::{
    crypto::{ Signature, Signer, VerifyingKey, signing_key_from_bytes },
    models::block::Block,
    traits::{ Hashable, ToBytes },
};
use rp_node::{ blockchain::Blockchain, contract::{ PeerId }, network::message::NetworkMessage };

use crate::{
    runtime::errors::RuntimeError,
    runtime::node::{ EventTx, NetworkCommand, NetworkRx, RuntimeEvent },
};

const TAG: &str = "manager";
const P256_SIGNATURE_COMPONENT_SIZE: usize = 32;
const UNCOMPRESSED_P256_PUBLIC_KEY_SIZE: usize = 65;
const P256_KEY_BITS: sys::psa_key_bits_t = 256;
const ESP32P4_ECDSA_MIN_SUPPORTED_CHIP_REVISION: u16 = 300;
const PROBE_SOFTWARE_SIGNING_KEY_BYTES: [u8; 32] = [7u8; 32];
const PROBE_KEY_PURPOSE: sys::esp_efuse_purpose_t =
    sys::esp_efuse_purpose_t_ESP_EFUSE_KEY_PURPOSE_ECDSA_KEY;
const PSA_SUCCESS: sys::psa_status_t = 0;
const PSA_ECC_FAMILY_SECP_R1: sys::psa_ecc_family_t = 0x12;
const PSA_KEY_TYPE_ECC_KEY_PAIR_BASE: sys::psa_key_type_t = 0x7100;
const PSA_KEY_USAGE_SIGN_HASH: sys::psa_key_usage_t = 0x0000_1000;
const PSA_ALG_SHA_256: sys::psa_algorithm_t = 0x0200_0009;
const PSA_ALG_ECDSA_BASE: sys::psa_algorithm_t = 0x0600_0600;
const PSA_KEY_LOCATION_ESP_ECDSA: sys::psa_key_location_t = 0x800001;
const PSA_KEY_PERSISTENCE_VOLATILE: sys::psa_key_persistence_t = 0x00;

fn psa_key_type_ecc_key_pair(curve: sys::psa_ecc_family_t) -> sys::psa_key_type_t {
    PSA_KEY_TYPE_ECC_KEY_PAIR_BASE | (curve as sys::psa_key_type_t)
}

fn psa_alg_ecdsa(hash_alg: sys::psa_algorithm_t) -> sys::psa_algorithm_t {
    PSA_ALG_ECDSA_BASE | (hash_alg & 0xff)
}

fn psa_key_lifetime_esp_ecdsa_volatile() -> sys::psa_key_lifetime_t {
    (PSA_KEY_LOCATION_ESP_ECDSA << 8) | (PSA_KEY_PERSISTENCE_VOLATILE as sys::psa_key_lifetime_t)
}

#[repr(C)]
struct EspChipInfo {
    model: core::ffi::c_int,
    features: u32,
    revision: u16,
    cores: u8,
}

unsafe extern "C" {
    fn esp_chip_info(out_info: *mut EspChipInfo);
}

pub struct NetworkManager {
    network_rx: NetworkRx,
    event_tx: EventTx,
    peer: PeerId,
}

impl NetworkManager {
    pub fn new(network_rx: NetworkRx, event_tx: EventTx, peer: PeerId) -> Self {
        Self { network_rx, event_tx, peer }
    }

    pub async fn run(&mut self) -> Result<(), RuntimeError> {
        info!(target: TAG, "Running Network");

        match build_probe_block() {
            Ok(probe_block) => {
                info!(
                    target: TAG,
                    "injecting probe block height={} slot={}",
                    probe_block.height,
                    probe_block.slot
                );

                self.event_tx
                    .send(RuntimeEvent::FrameReceived {
                        peer: self.peer,
                        frame: NetworkMessage::NewBlock(probe_block).to_bytes(),
                    }).await
                    .map_err(RuntimeError::event_send)?;
            }
            Err(error) => {
                error!(target: TAG, "probe block setup failed: {:?}", error);
            }
        }

        while let Some(command) = self.network_rx.next().await {
            match command {
                NetworkCommand::SendFrame { peer, .. } => {
                    info!(target: TAG, "send frame to peer: {:?}", peer);
                }
                NetworkCommand::BroadcastFrame { .. } => {
                    info!(target: TAG, "broadcast frame");
                }
                NetworkCommand::DisconnectPeer { peer } => {
                    info!(target: TAG, "disconnect peer: {:?}", peer);
                }
                NetworkCommand::RequestBlocks { peer, from_height, to_height } => {
                    info!(
                        target: TAG,
                        "request blocks from peer {:?}: {}..{}",
                        peer,
                        from_height,
                        to_height
                    );
                }
            }
        }

        Ok(())
    }
}

fn build_probe_block() -> Result<Block, RuntimeError> {
    let blockchain = Blockchain::new().expect("probe blockchain should initialize");
    let parent = blockchain.get_latest_block().clone();
    let parent_state = blockchain.state.clone();
    info!(target: TAG, "probing efuse for ECDSA_KEY block");
    let (validator, key_block) = match find_ecdsa_key_block() {
        Ok(key_block) => {
            if let Some((major, minor)) = unsupported_esp32p4_ecdsa_revision() {
                warn!(
                    target: TAG,
                    "efuse ECDSA key found in block {}, but ESP32-P4 hardware ECDSA requires chip revision >= v3.0; current chip revision is v{}.{}; falling back to software probe signer",
                    key_block,
                    major,
                    minor
                );
                let signing_key = signing_key_from_bytes(&PROBE_SOFTWARE_SIGNING_KEY_BYTES).map_err(
                    RuntimeError::crypto
                )?;
                (signing_key.verifying_key().clone(), None)
            } else {
                info!(target: TAG, "exporting validator public key from efuse block {}", key_block);
                let validator = load_verifying_key_from_efuse(key_block).map_err(
                    RuntimeError::crypto
                )?;
                info!(target: TAG, "using efuse ECDSA key block {}", key_block);
                (validator, Some(key_block))
            }
        }
        Err("no efuse key with ECDSA_KEY purpose found") => {
            warn!(
                target: TAG,
                "no efuse ECDSA key provisioned; falling back to software probe signer"
            );
            let signing_key = signing_key_from_bytes(&PROBE_SOFTWARE_SIGNING_KEY_BYTES).map_err(
                RuntimeError::crypto
            )?;
            (signing_key.verifying_key().clone(), None)
        }
        Err(error) => {
            return Err(RuntimeError::crypto(error));
        }
    };

    let mut block = Block {
        height: parent.height + 1,
        slot: parent.slot + 1,
        previous_hash: parent.hash(),
        validator,
        transactions: vec![],
        signature: None,
        slash_proofs: vec![],
        state_root: parent_state.compute_state_root(),
    };
    let hash = block.hash();
    block.signature = Some(match key_block {
        Some(key_block) => {
            info!(target: TAG, "signing probe block hash with efuse block {}", key_block);
            sign_hash_with_psa(&hash, key_block).map_err(RuntimeError::crypto)?
        }
        None => {
            info!(target: TAG, "signing probe block hash with software fallback signer");
            sign_hash_with_software(&hash).map_err(RuntimeError::crypto)?
        }
    });
    Ok(block)
}

fn find_ecdsa_key_block() -> Result<sys::esp_efuse_block_t, &'static str> {
    let mut block = sys::esp_efuse_block_t_EFUSE_BLK_KEY_MAX;

    let found = unsafe { sys::esp_efuse_find_purpose(PROBE_KEY_PURPOSE, &mut block) };

    if found {
        Ok(block)
    } else {
        Err("no efuse key with ECDSA_KEY purpose found")
    }
}

fn unsupported_esp32p4_ecdsa_revision() -> Option<(u16, u16)> {
    let chip_revision = unsafe {
        let mut chip_info = core::mem::MaybeUninit::<EspChipInfo>::zeroed();
        esp_chip_info(chip_info.as_mut_ptr());
        chip_info.assume_init().revision
    };

    if chip_revision >= ESP32P4_ECDSA_MIN_SUPPORTED_CHIP_REVISION {
        None
    } else {
        Some((chip_revision / 100, chip_revision % 100))
    }
}

fn load_verifying_key_from_efuse(
    efuse_block: sys::esp_efuse_block_t
) -> Result<VerifyingKey, &'static str> {
    let key = import_psa_opaque_key(efuse_block)?;

    unsafe {
        let mut public_key_bytes = [0u8; UNCOMPRESSED_P256_PUBLIC_KEY_SIZE];
        let mut public_key_len = 0usize;

        psa_ok(
            sys::psa_export_public_key(
                key.id,
                public_key_bytes.as_mut_ptr(),
                public_key_bytes.len(),
                &mut public_key_len
            )
        ).map_err(|_| "failed to export public key from opaque PSA ECDSA key")?;

        VerifyingKey::from_sec1_bytes(&public_key_bytes[..public_key_len]).map_err(
            |_| "invalid P-256 public key exported from efuse"
        )
    }
}

fn sign_hash_with_psa(
    hash: &[u8],
    efuse_block: sys::esp_efuse_block_t
) -> Result<Signature, &'static str> {
    let key = import_psa_opaque_key(efuse_block)?;

    unsafe {
        let mut signature_bytes = [0u8; 64];
        let mut signature_len = 0usize;

        psa_ok(
            sys::psa_sign_hash(
                key.id,
                psa_alg_ecdsa(PSA_ALG_SHA_256),
                hash.as_ptr(),
                hash.len(),
                signature_bytes.as_mut_ptr(),
                signature_bytes.len(),
                &mut signature_len
            )
        ).map_err(|_| "psa_sign_hash failed for opaque ECDSA key")?;

        if signature_len != P256_SIGNATURE_COMPONENT_SIZE * 2 {
            return Err("opaque ECDSA signer returned unexpected signature length");
        }

        Signature::from_slice(&signature_bytes).map_err(|_| "invalid p256 signature bytes")
    }
}

fn sign_hash_with_software(hash: &[u8]) -> Result<Signature, &'static str> {
    let signing_key = signing_key_from_bytes(&PROBE_SOFTWARE_SIGNING_KEY_BYTES)?;
    Ok(signing_key.sign(hash))
}

fn import_psa_opaque_key(efuse_block: sys::esp_efuse_block_t) -> Result<PsaKey, &'static str> {
    unsafe {
        psa_ok(sys::psa_crypto_init()).map_err(|_| "psa_crypto_init failed")?;

        let mut attributes = sys::psa_key_attributes_t::default();
        attributes.type_ = psa_key_type_ecc_key_pair(PSA_ECC_FAMILY_SECP_R1);
        attributes.bits = P256_KEY_BITS;
        attributes.lifetime = psa_key_lifetime_esp_ecdsa_volatile();
        attributes.policy.usage = PSA_KEY_USAGE_SIGN_HASH;
        attributes.policy.alg = psa_alg_ecdsa(PSA_ALG_SHA_256);
        attributes.policy.alg2 = 0;
        attributes.id = 0;

        let opaque_key = sys::esp_ecdsa_opaque_key_t {
            curve: sys::esp_ecdsa_curve_t_ESP_ECDSA_CURVE_SECP256R1,
            use_km_key: false,
            efuse_block: efuse_block as u8,
        };

        let mut key_id = 0;
        psa_ok(
            sys::psa_import_key(
                &attributes,
                (&opaque_key as *const sys::esp_ecdsa_opaque_key_t).cast(),
                core::mem::size_of::<sys::esp_ecdsa_opaque_key_t>(),
                &mut key_id
            )
        ).map_err(|_| "failed to import opaque efuse ECDSA key into PSA")?;

        if key_id == 0 {
            return Err("PSA returned an invalid opaque ECDSA key id");
        }

        Ok(PsaKey { id: key_id })
    }
}

fn psa_ok(status: sys::psa_status_t) -> Result<(), sys::psa_status_t> {
    if status == PSA_SUCCESS { Ok(()) } else { Err(status) }
}

struct PsaKey {
    id: sys::mbedtls_svc_key_id_t,
}

impl Drop for PsaKey {
    fn drop(&mut self) {
        unsafe {
            let _ = sys::psa_destroy_key(self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use rp_node::contract::{ NodeAction, NodeInput };
    use rp_node::node_engine::NodeEngine;

    #[test]
    fn test_probe_block_frame_is_accepted_by_node_engine() {
        let mut engine = NodeEngine::new(Blockchain::new().unwrap());
        let actions = engine.step(NodeInput::FrameReceived {
            peer: [0u8; 32],
            frame: NetworkMessage::NewBlock(
                build_probe_block().expect("probe block should load an efuse-backed ECDSA key")
            ).to_bytes(),
        });

        assert_eq!(actions.len(), 2);
        assert!(matches!(actions[0], NodeAction::PersistBlock { .. }));
        assert!(matches!(actions[1], NodeAction::PersistSnapshot { .. }));
    }
}
