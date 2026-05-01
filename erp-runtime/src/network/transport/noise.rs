use std::{ io::{ self, Error, ErrorKind }, pin::Pin, task::{ Context, Poll, ready } };

use futures::io::{ AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt };
use libp2p_identity::PublicKey;
use quick_protobuf::{
    BytesReader,
    MessageRead,
    MessageWrite,
    Writer,
    WriterBackend,
    sizeofs::sizeof_len,
};
use snow::{ HandshakeState, TransportState };

use crate::{ network::transport_identity::TransportIdentityManager, runtime::errors::RuntimeError };

pub const NOISE_PROTOCOL: &str = "/noise";
pub const NOISE_PROTOCOL_NAME: &str = "Noise_XX_25519_ChaChaPoly_SHA256";
const STATIC_KEY_DOMAIN: &str = "noise-libp2p-static-key:";
const NOISE_TAG_LEN: usize = 16;
const MAX_NOISE_FRAME_LEN: usize = u16::MAX as usize;
const MAX_NOISE_PLAINTEXT_LEN: usize = MAX_NOISE_FRAME_LEN - NOISE_TAG_LEN;

pub struct NoiseUpgradeOutput<S> {
    pub stream: S,
    pub remote_transport_peer_id: Vec<u8>,
    pub remote_transport_public_key: Vec<u8>,
}

pub struct NoiseStream<S> {
    inner: S,
    session: TransportState,
    read_len_buf: [u8; 2],
    read_len_pos: usize,
    read_frame_len: Option<usize>,
    read_encrypted: Vec<u8>,
    read_encrypted_pos: usize,
    read_plaintext: Vec<u8>,
    read_plaintext_pos: usize,
    write_frame: Vec<u8>,
    write_frame_pos: usize,
}

impl<S> NoiseStream<S> {
    fn new(inner: S, session: TransportState) -> Self {
        Self {
            inner,
            session,
            read_len_buf: [0u8; 2],
            read_len_pos: 0,
            read_frame_len: None,
            read_encrypted: Vec::new(),
            read_encrypted_pos: 0,
            read_plaintext: Vec::new(),
            read_plaintext_pos: 0,
            write_frame: Vec::new(),
            write_frame_pos: 0,
        }
    }

    fn poll_fill_plaintext(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<bool>>
        where S: AsyncRead + Unpin
    {
        loop {
            if self.read_plaintext_pos < self.read_plaintext.len() {
                return Poll::Ready(Ok(true));
            }

            self.read_plaintext.clear();
            self.read_plaintext_pos = 0;

            if self.read_frame_len.is_none() {
                while self.read_len_pos < self.read_len_buf.len() {
                    match
                        ready!(
                            Pin::new(&mut self.inner).poll_read(
                                cx,
                                &mut self.read_len_buf[self.read_len_pos..]
                            )
                        )
                    {
                        Ok(0) if self.read_len_pos == 0 => {
                            return Poll::Ready(Ok(false));
                        }
                        Ok(0) => {
                            return Poll::Ready(
                                Err(
                                    Error::new(
                                        ErrorKind::UnexpectedEof,
                                        "noise frame ended while reading length prefix"
                                    )
                                )
                            );
                        }
                        Ok(read) => {
                            self.read_len_pos += read;
                        }
                        Err(error) => {
                            return Poll::Ready(Err(error));
                        }
                    }
                }

                let frame_len = u16::from_be_bytes(self.read_len_buf) as usize;
                self.read_frame_len = Some(frame_len);
                self.read_encrypted.clear();
                self.read_encrypted.resize(frame_len, 0);
                self.read_encrypted_pos = 0;
            }

            let frame_len = self.read_frame_len.expect(
                "frame length is always set before body reads begin"
            );
            while self.read_encrypted_pos < frame_len {
                match
                    ready!(
                        Pin::new(&mut self.inner).poll_read(
                            cx,
                            &mut self.read_encrypted[self.read_encrypted_pos..frame_len]
                        )
                    )
                {
                    Ok(0) => {
                        return Poll::Ready(
                            Err(
                                Error::new(
                                    ErrorKind::UnexpectedEof,
                                    "noise frame ended while reading ciphertext"
                                )
                            )
                        );
                    }
                    Ok(read) => {
                        self.read_encrypted_pos += read;
                    }
                    Err(error) => {
                        return Poll::Ready(Err(error));
                    }
                }
            }

            let mut plaintext = vec![0u8; frame_len];
            let plaintext_len = self.session
                .read_message(&self.read_encrypted, &mut plaintext)
                .map_err(noise_to_io_error)?;
            plaintext.truncate(plaintext_len);

            self.read_plaintext = plaintext;
            self.read_plaintext_pos = 0;
            self.read_frame_len = None;
            self.read_len_pos = 0;
            self.read_encrypted.clear();
            self.read_encrypted_pos = 0;

            if !self.read_plaintext.is_empty() {
                return Poll::Ready(Ok(true));
            }
        }
    }

    fn queue_encrypted_frame(&mut self, plaintext: &[u8]) -> io::Result<usize> {
        let plaintext_len = plaintext.len().min(MAX_NOISE_PLAINTEXT_LEN);
        let mut ciphertext = vec![0u8; plaintext_len + NOISE_TAG_LEN];
        let ciphertext_len = self.session
            .write_message(&plaintext[..plaintext_len], &mut ciphertext)
            .map_err(noise_to_io_error)?;

        self.write_frame.clear();
        self.write_frame.extend_from_slice(&(ciphertext_len as u16).to_be_bytes());
        self.write_frame.extend_from_slice(&ciphertext[..ciphertext_len]);
        self.write_frame_pos = 0;

        Ok(plaintext_len)
    }

    fn poll_flush_pending_frame(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>>
        where S: AsyncWrite + Unpin
    {
        while self.write_frame_pos < self.write_frame.len() {
            let written = ready!(
                Pin::new(&mut self.inner).poll_write(cx, &self.write_frame[self.write_frame_pos..])
            )?;

            if written == 0 {
                return Poll::Ready(
                    Err(Error::new(ErrorKind::WriteZero, "failed to flush buffered noise frame"))
                );
            }

            self.write_frame_pos += written;
        }

        self.write_frame.clear();
        self.write_frame_pos = 0;
        Poll::Ready(Ok(()))
    }
}

impl<S> AsyncRead for NoiseStream<S> where S: AsyncRead + Unpin {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8]
    ) -> Poll<io::Result<usize>> {
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }

        match ready!(self.poll_fill_plaintext(cx)) {
            Ok(false) => Poll::Ready(Ok(0)),
            Ok(true) => {
                let available = &self.read_plaintext[self.read_plaintext_pos..];
                let len = available.len().min(buf.len());
                buf[..len].copy_from_slice(&available[..len]);
                self.read_plaintext_pos += len;
                if self.read_plaintext_pos == self.read_plaintext.len() {
                    self.read_plaintext.clear();
                    self.read_plaintext_pos = 0;
                }
                Poll::Ready(Ok(len))
            }
            Err(error) => Poll::Ready(Err(error)),
        }
    }
}

impl<S> AsyncWrite for NoiseStream<S> where S: AsyncWrite + Unpin {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8]
    ) -> Poll<io::Result<usize>> {
        if !self.write_frame.is_empty() {
            match self.poll_flush_pending_frame(cx) {
                Poll::Ready(Ok(())) => {}
                Poll::Ready(Err(error)) => {
                    return Poll::Ready(Err(error));
                }
                Poll::Pending => {
                    return Poll::Pending;
                }
            }
        }

        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }

        let consumed = self.queue_encrypted_frame(buf)?;
        match self.poll_flush_pending_frame(cx) {
            Poll::Ready(Ok(())) | Poll::Pending => Poll::Ready(Ok(consumed)),
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.poll_flush_pending_frame(cx) {
            Poll::Ready(Ok(())) => Pin::new(&mut self.inner).poll_flush(cx),
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.poll_flush_pending_frame(cx) {
            Poll::Ready(Ok(())) => Pin::new(&mut self.inner).poll_close(cx),
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Pending => Poll::Pending,
        }
    }
}

pub async fn upgrade_outbound<S>(
    stream: S,
    identity: &TransportIdentityManager
) -> Result<NoiseUpgradeOutput<NoiseStream<S>>, RuntimeError>
    where S: AsyncRead + AsyncWrite + Unpin
{
    let local_static = generate_local_static_keypair()?;
    let local_payload = build_local_identity_payload(&local_static.public, identity)?;
    let mut handshake = build_handshake_state(true, &local_static.private)?;
    let mut stream = stream;

    send_handshake_payload(&mut stream, &mut handshake, &NoiseHandshakePayload::default()).await?;
    let remote_payload = recv_handshake_payload(&mut stream, &mut handshake).await?;
    let remote_identity = verify_remote_identity(&handshake, &remote_payload)?;
    send_handshake_payload(&mut stream, &mut handshake, &local_payload).await?;

    let transport = handshake.into_transport_mode().map_err(noise_to_runtime_error)?;

    Ok(NoiseUpgradeOutput {
        stream: NoiseStream::new(stream, transport),
        remote_transport_peer_id: remote_identity.to_peer_id().to_bytes(),
        remote_transport_public_key: remote_payload.identity_key,
    })
}

pub async fn upgrade_inbound<S>(
    stream: S,
    identity: &TransportIdentityManager
) -> Result<NoiseUpgradeOutput<NoiseStream<S>>, RuntimeError>
    where S: AsyncRead + AsyncWrite + Unpin
{
    let local_static = generate_local_static_keypair()?;
    let local_payload = build_local_identity_payload(&local_static.public, identity)?;
    let mut handshake = build_handshake_state(false, &local_static.private)?;
    let mut stream = stream;

    let first_payload = recv_handshake_payload(&mut stream, &mut handshake).await?;
    if !first_payload.is_empty() {
        return Err(RuntimeError::config("noise initiator sent a non-empty first payload"));
    }

    send_handshake_payload(&mut stream, &mut handshake, &local_payload).await?;
    let remote_payload = recv_handshake_payload(&mut stream, &mut handshake).await?;
    let remote_identity = verify_remote_identity(&handshake, &remote_payload)?;

    let transport = handshake.into_transport_mode().map_err(noise_to_runtime_error)?;

    Ok(NoiseUpgradeOutput {
        stream: NoiseStream::new(stream, transport),
        remote_transport_peer_id: remote_identity.to_peer_id().to_bytes(),
        remote_transport_public_key: remote_payload.identity_key,
    })
}

#[derive(Debug, Clone)]
struct LocalStaticKeypair {
    private: Vec<u8>,
    public: Vec<u8>,
}

#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
struct NoiseExtensions {
    webtransport_certhashes: Vec<Vec<u8>>,
    stream_muxers: Vec<String>,
}

#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
struct NoiseHandshakePayload {
    identity_key: Vec<u8>,
    identity_sig: Vec<u8>,
    extensions: Option<NoiseExtensions>,
}

impl NoiseHandshakePayload {
    fn is_empty(&self) -> bool {
        self.identity_key.is_empty() && self.identity_sig.is_empty() && self.extensions.is_none()
    }
}

impl<'a> MessageRead<'a> for NoiseExtensions {
    fn from_reader(reader: &mut BytesReader, bytes: &'a [u8]) -> quick_protobuf::Result<Self> {
        let mut message = Self::default();
        while !reader.is_eof() {
            match reader.next_tag(bytes) {
                Ok(10) =>
                    message.webtransport_certhashes.push(reader.read_bytes(bytes)?.to_owned()),
                Ok(18) => message.stream_muxers.push(reader.read_string(bytes)?.to_owned()),
                Ok(tag) => reader.read_unknown(bytes, tag)?,
                Err(error) => {
                    return Err(error);
                }
            }
        }
        Ok(message)
    }
}

impl MessageWrite for NoiseExtensions {
    fn get_size(&self) -> usize {
        self.webtransport_certhashes
            .iter()
            .map(|value| 1 + sizeof_len(value.len()))
            .sum::<usize>() +
            self.stream_muxers
                .iter()
                .map(|value| 1 + sizeof_len(value.len()))
                .sum::<usize>()
    }

    fn write_message<W: WriterBackend>(
        &self,
        writer: &mut Writer<W>
    ) -> quick_protobuf::Result<()> {
        for value in &self.webtransport_certhashes {
            writer.write_with_tag(10, |writer| writer.write_bytes(value))?;
        }
        for value in &self.stream_muxers {
            writer.write_with_tag(18, |writer| writer.write_string(value))?;
        }
        Ok(())
    }
}

impl<'a> MessageRead<'a> for NoiseHandshakePayload {
    fn from_reader(reader: &mut BytesReader, bytes: &'a [u8]) -> quick_protobuf::Result<Self> {
        let mut message = Self::default();
        while !reader.is_eof() {
            match reader.next_tag(bytes) {
                Ok(10) => {
                    message.identity_key = reader.read_bytes(bytes)?.to_owned();
                }
                Ok(18) => {
                    message.identity_sig = reader.read_bytes(bytes)?.to_owned();
                }
                Ok(34) => {
                    message.extensions = Some(reader.read_message::<NoiseExtensions>(bytes)?);
                }
                Ok(tag) => reader.read_unknown(bytes, tag)?,
                Err(error) => {
                    return Err(error);
                }
            }
        }
        Ok(message)
    }
}

impl MessageWrite for NoiseHandshakePayload {
    fn get_size(&self) -> usize {
        let identity_key_size = if self.identity_key.is_empty() {
            0
        } else {
            1 + sizeof_len(self.identity_key.len())
        };
        let identity_sig_size = if self.identity_sig.is_empty() {
            0
        } else {
            1 + sizeof_len(self.identity_sig.len())
        };
        let extensions_size = self.extensions
            .as_ref()
            .map_or(0, |extensions| 1 + sizeof_len(extensions.get_size()));

        identity_key_size + identity_sig_size + extensions_size
    }

    fn write_message<W: WriterBackend>(
        &self,
        writer: &mut Writer<W>
    ) -> quick_protobuf::Result<()> {
        if !self.identity_key.is_empty() {
            writer.write_with_tag(10, |writer| writer.write_bytes(&self.identity_key))?;
        }
        if !self.identity_sig.is_empty() {
            writer.write_with_tag(18, |writer| writer.write_bytes(&self.identity_sig))?;
        }
        if let Some(extensions) = &self.extensions {
            writer.write_with_tag(34, |writer| writer.write_message(extensions))?;
        }
        Ok(())
    }
}

fn generate_local_static_keypair() -> Result<LocalStaticKeypair, RuntimeError> {
    let params = NOISE_PROTOCOL_NAME.parse().map_err(|_|
        RuntimeError::config("invalid noise protocol parameters")
    )?;
    let builder = snow::Builder::new(params);
    let keypair = builder.generate_keypair().map_err(noise_to_runtime_error)?;

    Ok(LocalStaticKeypair {
        private: keypair.private,
        public: keypair.public,
    })
}

fn build_handshake_state(
    initiator: bool,
    local_private_key: &[u8]
) -> Result<HandshakeState, RuntimeError> {
    let params = NOISE_PROTOCOL_NAME.parse().map_err(|_|
        RuntimeError::config("invalid noise protocol parameters")
    )?;
    let builder = snow::Builder::new(params).local_private_key(local_private_key);

    if initiator {
        builder.build_initiator().map_err(noise_to_runtime_error)
    } else {
        builder.build_responder().map_err(noise_to_runtime_error)
    }
}

fn build_local_identity_payload(
    local_static_public_key: &[u8],
    identity: &TransportIdentityManager
) -> Result<NoiseHandshakePayload, RuntimeError> {
    let signature_input = [STATIC_KEY_DOMAIN.as_bytes(), local_static_public_key].concat();

    Ok(NoiseHandshakePayload {
        identity_key: identity.public_key_protobuf_bytes().to_vec(),
        identity_sig: identity.sign(&signature_input)?,
        extensions: None,
    })
}

fn verify_remote_identity(
    handshake: &HandshakeState,
    payload: &NoiseHandshakePayload
) -> Result<PublicKey, RuntimeError> {
    let remote_static_public_key = handshake
        .get_remote_static()
        .ok_or_else(|| RuntimeError::config("noise handshake did not expose a remote static key"))?;

    if payload.identity_key.is_empty() {
        return Err(
            RuntimeError::config("noise handshake payload is missing a transport public key")
        );
    }
    if payload.identity_sig.is_empty() {
        return Err(
            RuntimeError::config("noise handshake payload is missing a transport signature")
        );
    }

    let public_key = PublicKey::try_decode_protobuf(&payload.identity_key).map_err(|_|
        RuntimeError::crypto("noise handshake payload contains an invalid transport public key")
    )?;
    let signature_input = [STATIC_KEY_DOMAIN.as_bytes(), remote_static_public_key].concat();

    if !public_key.verify(&signature_input, &payload.identity_sig) {
        return Err(RuntimeError::config("noise handshake transport signature verification failed"));
    }

    Ok(public_key)
}

async fn send_handshake_payload<S>(
    stream: &mut S,
    handshake: &mut HandshakeState,
    payload: &NoiseHandshakePayload
) -> Result<(), RuntimeError>
    where S: AsyncWrite + Unpin
{
    let payload_bytes = encode_payload(payload)?;
    let mut ciphertext = vec![0u8; MAX_NOISE_FRAME_LEN];
    let ciphertext_len = handshake
        .write_message(&payload_bytes, &mut ciphertext)
        .map_err(noise_to_runtime_error)?;

    stream
        .write_all(&(ciphertext_len as u16).to_be_bytes()).await
        .map_err(RuntimeError::NetworkError)?;
    stream.write_all(&ciphertext[..ciphertext_len]).await.map_err(RuntimeError::NetworkError)?;
    stream.flush().await.map_err(RuntimeError::NetworkError)
}

async fn recv_handshake_payload<S>(
    stream: &mut S,
    handshake: &mut HandshakeState
) -> Result<NoiseHandshakePayload, RuntimeError>
    where S: AsyncRead + Unpin
{
    let mut len_bytes = [0u8; 2];
    stream.read_exact(&mut len_bytes).await.map_err(RuntimeError::NetworkError)?;
    let frame_len = u16::from_be_bytes(len_bytes) as usize;

    let mut ciphertext = vec![0u8; frame_len];
    stream.read_exact(&mut ciphertext).await.map_err(RuntimeError::NetworkError)?;

    let mut plaintext = vec![0u8; frame_len];
    let plaintext_len = handshake
        .read_message(&ciphertext, &mut plaintext)
        .map_err(noise_to_runtime_error)?;
    plaintext.truncate(plaintext_len);

    decode_payload(&plaintext)
}

fn encode_payload(payload: &NoiseHandshakePayload) -> Result<Vec<u8>, RuntimeError> {
    let mut bytes = Vec::with_capacity(payload.get_size());
    payload
        .write_message(&mut Writer::new(&mut bytes))
        .map_err(|_| RuntimeError::crypto("failed to encode noise handshake payload"))?;
    Ok(bytes)
}

fn decode_payload(bytes: &[u8]) -> Result<NoiseHandshakePayload, RuntimeError> {
    let mut reader = BytesReader::from_bytes(bytes);
    NoiseHandshakePayload::from_reader(&mut reader, bytes).map_err(|_|
        RuntimeError::crypto("failed to decode noise handshake payload")
    )
}

fn noise_to_runtime_error(error: snow::Error) -> RuntimeError {
    RuntimeError::NetworkError(noise_to_io_error(error))
}

fn noise_to_io_error(error: snow::Error) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, format!("noise transport error: {error}"))
}
