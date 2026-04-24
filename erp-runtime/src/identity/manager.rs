use esp_idf_hal::sys;
use log::{ info, warn };
use rp_core::{
    crypto::{
        Signature,
        Signer,
        SigningKey,
        VerifyingKey,
        signing_key_from_bytes,
        verifying_key_bytes,
    },
    traits::Hashable,
};
use rp_node::{ contract::{ Identity, PeerId }, errors::ContractError };

use crate::runtime::errors::RuntimeError;

const TAG: &str = "identity";
const DEVELOPMENT_IDENTITY_SIGNING_KEY_BYTES: [u8; 32] = [7u8; 32];
const UNCOMPRESSED_P256_PUBLIC_KEY_SIZE: usize = 65;
const P256_SIGNATURE_COMPONENT_SIZE: usize = 32;
const P256_KEY_BITS: sys::psa_key_bits_t = 256;
const ESP32P4_ECDSA_MIN_SUPPORTED_CHIP_REVISION: u16 = 300;
const ECDSA_KEY_PURPOSE: sys::esp_efuse_purpose_t =
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

enum IdentityBackend {
    Software(SigningKey),
    Efuse {
        key_block: sys::esp_efuse_block_t,
    },
}

pub struct IdentityManager {
    peer_id: PeerId,
    public_key: Vec<u8>,
    verifying_key: VerifyingKey,
    backend: IdentityBackend,
}

impl IdentityManager {
    pub fn new(signing_key: SigningKey) -> Self {
        Self::from_software_signing_key(signing_key)
    }

    pub fn select() -> Result<Self, RuntimeError> {
        info!(target: TAG, "selecting runtime identity");

        match find_ecdsa_key_block() {
            Ok(key_block) => {
                if let Some((major, minor)) = unsupported_esp32p4_ecdsa_revision() {
                    warn!(
                        target: TAG,
                        "efuse ECDSA key found in block {}, but ESP32-P4 hardware ECDSA requires chip revision >= v3.0; current chip revision is v{}.{}; falling back to development software identity",
                        key_block,
                        major,
                        minor
                    );
                    Self::development()
                } else {
                    info!(target: TAG, "using efuse identity from key block {}", key_block);
                    Self::from_efuse_block(key_block)
                }
            }
            Err("no efuse key with ECDSA_KEY purpose found") => {
                warn!(
                    target: TAG,
                    "no efuse ECDSA identity provisioned; falling back to development software identity"
                );
                Self::development()
            }
            Err(error) => Err(RuntimeError::crypto(error)),
        }
    }

    fn development() -> Result<Self, RuntimeError> {
        let signing_key = signing_key_from_bytes(&DEVELOPMENT_IDENTITY_SIGNING_KEY_BYTES).map_err(
            RuntimeError::crypto
        )?;

        Ok(Self::from_software_signing_key(signing_key))
    }

    fn from_software_signing_key(signing_key: SigningKey) -> Self {
        let verifying_key = signing_key.verifying_key().clone();
        let public_key = verifying_key_bytes(&verifying_key);
        let peer_id = public_key.hash();

        Self {
            peer_id,
            public_key,
            verifying_key,
            backend: IdentityBackend::Software(signing_key),
        }
    }

    fn from_efuse_block(key_block: sys::esp_efuse_block_t) -> Result<Self, RuntimeError> {
        let verifying_key = load_verifying_key_from_efuse(key_block).map_err(RuntimeError::crypto)?;
        let public_key = verifying_key_bytes(&verifying_key);
        let peer_id = public_key.hash();

        Ok(Self {
            peer_id,
            public_key,
            verifying_key,
            backend: IdentityBackend::Efuse { key_block },
        })
    }

    pub fn verifying_key(&self) -> &VerifyingKey {
        &self.verifying_key
    }
}

impl Identity for IdentityManager {
    fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    fn public_key(&self) -> Vec<u8> {
        self.public_key.clone()
    }

    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, ContractError> {
        let signature = match &self.backend {
            IdentityBackend::Software(signing_key) => signing_key.sign(message),
            IdentityBackend::Efuse { key_block } => {
                sign_hash_with_psa(message, *key_block).map_err(|_| ContractError::Identity)?
            }
        };

        Ok(signature.to_bytes().to_vec())
    }
}

fn find_ecdsa_key_block() -> Result<sys::esp_efuse_block_t, &'static str> {
    let mut block = sys::esp_efuse_block_t_EFUSE_BLK_KEY_MAX;

    let found = unsafe { sys::esp_efuse_find_purpose(ECDSA_KEY_PURPOSE, &mut block) };

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
    use rp_core::crypto::{ Signature, Verifier, signing_key_from_bytes, verifying_key_bytes };

    #[test]
    fn new_derives_public_key_and_peer_id() {
        let signing_key = signing_key_from_bytes(&[3u8; 32]).unwrap();
        let manager = IdentityManager::new(signing_key);

        assert_eq!(manager.public_key(), verifying_key_bytes(manager.verifying_key()));
        assert_eq!(manager.peer_id(), manager.public_key().hash());
    }

    #[test]
    fn sign_returns_signature_for_verifying_key() {
        let signing_key = signing_key_from_bytes(&[9u8; 32]).unwrap();
        let manager = IdentityManager::new(signing_key);
        let message = b"embedded identity";

        let signature_bytes = manager.sign(message).unwrap();
        let signature = Signature::from_slice(&signature_bytes).unwrap();

        manager.verifying_key().verify(message, &signature).unwrap();
    }
}
