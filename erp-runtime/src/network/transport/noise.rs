use crate::{ network::transport_identity::TransportIdentityManager, runtime::errors::RuntimeError };

pub const NOISE_PROTOCOL: &str = "/noise";
pub const NOISE_PROTOCOL_NAME: &str = "Noise_XX_25519_ChaChaPoly_SHA256";

pub struct NoiseUpgradeOutput<S> {
    pub stream: S,
    pub remote_transport_peer_id: Vec<u8>,
    pub remote_transport_public_key: Vec<u8>,
}

pub async fn upgrade_outbound<S>(
    stream: S,
    identity: &TransportIdentityManager
) -> Result<NoiseUpgradeOutput<S>, RuntimeError> {
    let _ = (stream, identity);
    todo!("implement outbound noise upgrade")
}

pub async fn upgrade_inbound<S>(
    stream: S,
    identity: &TransportIdentityManager
) -> Result<NoiseUpgradeOutput<S>, RuntimeError> {
    let _ = (stream, identity);
    todo!("implement inbound noise upgrade")
}
