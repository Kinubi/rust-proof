use std::net::SocketAddr;

use multiaddr::{ Multiaddr, Protocol };
use quick_protobuf::{ BytesReader, MessageRead, MessageWrite, Writer, WriterBackend, sizeofs::sizeof_len };

use crate::{
    network::{
        codec::length_prefixed::{ decode_length_prefix, encode_length_prefixed },
        config::MultiaddrLite,
    },
    runtime::errors::RuntimeError,
};

pub const IDENTIFY_PROTOCOL: &str = "/ipfs/id/1.0.0";
const MAX_IDENTIFY_MESSAGE_SIZE: u32 = 4096;

#[derive(Debug, Clone)]
pub struct IdentifyInfo {
    pub protocol_version: String,
    pub agent_version: String,
    pub listen_addrs: Vec<MultiaddrLite>,
    pub supported_protocols: Vec<String>,
    pub observed_addr: Option<SocketAddr>,
    pub transport_public_key: Vec<u8>,
    pub transport_peer_id: Vec<u8>,
}

pub fn encode_identify(info: &IdentifyInfo) -> Result<Vec<u8>, RuntimeError> {
    let message = IdentifyMessage {
        protocolVersion: Some(info.protocol_version.clone()),
        agentVersion: Some(info.agent_version.clone()),
        publicKey: Some(info.transport_public_key.clone()),
        listenAddrs: info
            .listen_addrs
            .iter()
            .map(encode_multiaddr)
            .collect::<Result<Vec<_>, _>>()?,
        observedAddr: info.observed_addr.map(encode_socket_addr),
        protocols: info.supported_protocols.clone(),
        signedPeerRecord: None,
    };

    let mut payload = Vec::with_capacity(message.get_size());
    message
        .write_message(&mut Writer::new(&mut payload))
        .map_err(|_| RuntimeError::crypto("failed to encode identify protobuf payload"))?;

    encode_length_prefixed(&payload, MAX_IDENTIFY_MESSAGE_SIZE)
}

pub fn decode_identify(bytes: &[u8]) -> Result<IdentifyInfo, RuntimeError> {
    let (_, payload) = decode_length_prefix(bytes, MAX_IDENTIFY_MESSAGE_SIZE)?;
    let mut reader = BytesReader::from_bytes(payload);
    let message = IdentifyMessage::from_reader(&mut reader, payload)
        .map_err(|_| RuntimeError::crypto("failed to decode identify protobuf payload"))?;

    let transport_public_key = message
        .publicKey
        .ok_or_else(|| RuntimeError::config("identify payload is missing a transport public key"))?;
    let transport_public_key_decoded = libp2p_identity::PublicKey::try_decode_protobuf(
        &transport_public_key,
    )
    .map_err(|_| RuntimeError::crypto("identify payload contains an invalid transport public key"))?;

    Ok(IdentifyInfo {
        protocol_version: message.protocolVersion.unwrap_or_default(),
        agent_version: message.agentVersion.unwrap_or_default(),
        listen_addrs: message
            .listenAddrs
            .into_iter()
            .filter_map(|bytes| decode_multiaddr(&bytes).ok())
            .collect(),
        supported_protocols: message.protocols,
        observed_addr: message
            .observedAddr
            .as_deref()
            .and_then(|bytes| decode_socket_addr(bytes).ok()),
        transport_public_key,
        transport_peer_id: transport_public_key_decoded.to_peer_id().to_bytes(),
    })
}

#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
struct IdentifyMessage {
    protocolVersion: Option<String>,
    agentVersion: Option<String>,
    publicKey: Option<Vec<u8>>,
    listenAddrs: Vec<Vec<u8>>,
    observedAddr: Option<Vec<u8>>,
    protocols: Vec<String>,
    signedPeerRecord: Option<Vec<u8>>,
}

impl<'a> MessageRead<'a> for IdentifyMessage {
    fn from_reader(reader: &mut BytesReader, bytes: &'a [u8]) -> quick_protobuf::Result<Self> {
        let mut message = Self::default();

        while !reader.is_eof() {
            match reader.next_tag(bytes) {
                Ok(42) => message.protocolVersion = Some(reader.read_string(bytes)?.to_owned()),
                Ok(50) => message.agentVersion = Some(reader.read_string(bytes)?.to_owned()),
                Ok(10) => message.publicKey = Some(reader.read_bytes(bytes)?.to_owned()),
                Ok(18) => message.listenAddrs.push(reader.read_bytes(bytes)?.to_owned()),
                Ok(34) => message.observedAddr = Some(reader.read_bytes(bytes)?.to_owned()),
                Ok(26) => message.protocols.push(reader.read_string(bytes)?.to_owned()),
                Ok(66) => message.signedPeerRecord = Some(reader.read_bytes(bytes)?.to_owned()),
                Ok(tag) => reader.read_unknown(bytes, tag)?,
                Err(error) => return Err(error),
            }
        }

        Ok(message)
    }
}

impl MessageWrite for IdentifyMessage {
    fn get_size(&self) -> usize {
        self.protocolVersion.as_ref().map_or(0, |value| 1 + sizeof_len(value.len()))
            + self.agentVersion.as_ref().map_or(0, |value| 1 + sizeof_len(value.len()))
            + self.publicKey.as_ref().map_or(0, |value| 1 + sizeof_len(value.len()))
            + self.listenAddrs.iter().map(|value| 1 + sizeof_len(value.len())).sum::<usize>()
            + self.observedAddr.as_ref().map_or(0, |value| 1 + sizeof_len(value.len()))
            + self.protocols.iter().map(|value| 1 + sizeof_len(value.len())).sum::<usize>()
            + self.signedPeerRecord.as_ref().map_or(0, |value| 1 + sizeof_len(value.len()))
    }

    fn write_message<W: WriterBackend>(&self, writer: &mut Writer<W>) -> quick_protobuf::Result<()> {
        if let Some(value) = &self.protocolVersion {
            writer.write_with_tag(42, |writer| writer.write_string(value))?;
        }
        if let Some(value) = &self.agentVersion {
            writer.write_with_tag(50, |writer| writer.write_string(value))?;
        }
        if let Some(value) = &self.publicKey {
            writer.write_with_tag(10, |writer| writer.write_bytes(value))?;
        }
        for value in &self.listenAddrs {
            writer.write_with_tag(18, |writer| writer.write_bytes(value))?;
        }
        if let Some(value) = &self.observedAddr {
            writer.write_with_tag(34, |writer| writer.write_bytes(value))?;
        }
        for value in &self.protocols {
            writer.write_with_tag(26, |writer| writer.write_string(value))?;
        }
        if let Some(value) = &self.signedPeerRecord {
            writer.write_with_tag(66, |writer| writer.write_bytes(value))?;
        }

        Ok(())
    }
}

fn encode_multiaddr(addr: &MultiaddrLite) -> Result<Vec<u8>, RuntimeError> {
    let multiaddr = match addr {
        MultiaddrLite::Ip4Tcp { addr, port } => format!("/ip4/{}.{}.{}.{}/tcp/{port}", addr[0], addr[1], addr[2], addr[3]),
        MultiaddrLite::Dns4Tcp { host, port } => format!("/dns4/{host}/tcp/{port}"),
    }
    .parse::<Multiaddr>()
    .map_err(|_| RuntimeError::config("failed to encode identify listen multiaddr"))?;

    Ok(multiaddr.to_vec())
}

fn decode_multiaddr(bytes: &[u8]) -> Result<MultiaddrLite, RuntimeError> {
    let multiaddr = Multiaddr::try_from(bytes.to_vec())
        .map_err(|_| RuntimeError::config("failed to decode identify multiaddr bytes"))?;
    let mut protocols = multiaddr.iter();

    match (protocols.next(), protocols.next(), protocols.next()) {
        (Some(Protocol::Ip4(addr)), Some(Protocol::Tcp(port)), None) => {
            Ok(MultiaddrLite::Ip4Tcp { addr: addr.octets(), port })
        }
        (Some(Protocol::Dns4(host)), Some(Protocol::Tcp(port)), None) => {
            Ok(MultiaddrLite::Dns4Tcp { host: host.into_owned(), port })
        }
        _ => Err(RuntimeError::config("unsupported identify multiaddr")),
    }
}

fn encode_socket_addr(addr: SocketAddr) -> Vec<u8> {
    let multiaddr = match addr {
        SocketAddr::V4(addr) => format!("/ip4/{}/tcp/{}", addr.ip(), addr.port()),
        SocketAddr::V6(addr) => format!("/ip6/{}/tcp/{}", addr.ip(), addr.port()),
    }
    .parse::<Multiaddr>()
    .expect("socket address should always encode to a canonical multiaddr");

    multiaddr.to_vec()
}

fn decode_socket_addr(bytes: &[u8]) -> Result<SocketAddr, RuntimeError> {
    let multiaddr = Multiaddr::try_from(bytes.to_vec())
        .map_err(|_| RuntimeError::config("failed to decode observed multiaddr bytes"))?;
    let mut protocols = multiaddr.iter();

    match (protocols.next(), protocols.next(), protocols.next()) {
        (Some(Protocol::Ip4(addr)), Some(Protocol::Tcp(port)), None) => {
            Ok(SocketAddr::from((addr, port)))
        }
        (Some(Protocol::Ip6(addr)), Some(Protocol::Tcp(port)), None) => {
            Ok(SocketAddr::from((addr, port)))
        }
        _ => Err(RuntimeError::config("observed address is not a direct ip/tcp multiaddr")),
    }
}
