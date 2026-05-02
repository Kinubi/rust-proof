use std::io::{ Error, ErrorKind };

use futures::io::{ AsyncRead, AsyncWrite, AsyncWriteExt };
use log::debug;
use rp_codec::length_prefixed::{ decode_length_prefix, encode_length_prefixed, read_length_prefixed_frame };

use crate::runtime::errors::RuntimeError;

const TAG: &str = "multistream";
pub const MULTISTREAM_V1: &str = "/multistream/1.0.0";
const MULTISTREAM_NOT_AVAILABLE: &str = "na";
const MAX_PROTOCOL_FRAME_LEN: usize = 16 * 1024;

pub async fn write_protocol<S>(stream: &mut S, protocol: &str) -> Result<(), RuntimeError>
    where S: AsyncWrite + Unpin
{
    let mut line = protocol.as_bytes().to_vec();
    if !line.ends_with(b"\n") {
        line.push(b'\n');
    }
    if line.len() > MAX_PROTOCOL_FRAME_LEN {
        return Err(
            RuntimeError::NetworkError(
                Error::new(
                    ErrorKind::InvalidData,
                    "multistream protocol line exceeds maximum frame length"
                )
            )
        );
    }

    let frame = encode_length_prefixed(&line, MAX_PROTOCOL_FRAME_LEN as u32).map_err(
        RuntimeError::NetworkError
    )?;
    stream.write_all(&frame).await.map_err(RuntimeError::NetworkError)?;
    stream.flush().await.map_err(RuntimeError::NetworkError)
}

pub async fn read_protocol<S>(stream: &mut S, max_len: usize) -> Result<String, RuntimeError>
    where S: AsyncRead + Unpin
{
    let frame_limit = max_len.min(MAX_PROTOCOL_FRAME_LEN) as u32;
    let frame = read_length_prefixed_frame(stream, frame_limit).await.map_err(
        RuntimeError::NetworkError
    )?;
    let (_, payload) = decode_length_prefix(&frame, frame_limit).map_err(RuntimeError::NetworkError)?;
    let mut line = payload.to_vec();

    if !line.ends_with(b"\n") {
        return Err(
            RuntimeError::NetworkError(
                Error::new(
                    ErrorKind::InvalidData,
                    "multistream protocol line is not newline terminated"
                )
            )
        );
    }

    line.pop();
    String::from_utf8(line).map_err(|_| {
        RuntimeError::NetworkError(
            Error::new(ErrorKind::InvalidData, "multistream protocol line is not valid UTF-8")
        )
    })
}

pub async fn dialer_select<S>(stream: &mut S, protocol: &str) -> Result<(), RuntimeError>
    where S: AsyncRead + AsyncWrite + Unpin
{
    debug!(target: TAG, "dialer_select: sending multistream header");
    write_protocol(stream, MULTISTREAM_V1).await?;
    debug!(target: TAG, "dialer_select: waiting for remote header");
    let remote_header = read_protocol(stream, MAX_PROTOCOL_FRAME_LEN).await?;
    debug!(target: TAG, "dialer_select: remote header = {:?}", remote_header);
    if remote_header != MULTISTREAM_V1 {
        return Err(RuntimeError::config("remote did not acknowledge multistream v1"));
    }

    debug!(target: TAG, "dialer_select: sending protocol {:?}", protocol);
    write_protocol(stream, protocol).await?;
    debug!(target: TAG, "dialer_select: waiting for protocol selection");
    let selected = read_protocol(stream, MAX_PROTOCOL_FRAME_LEN).await?;
    debug!(target: TAG, "dialer_select: selected = {:?}", selected);
    if selected == protocol {
        return Ok(());
    }

    if selected == MULTISTREAM_NOT_AVAILABLE {
        return Err(RuntimeError::config("remote rejected requested protocol"));
    }

    Err(RuntimeError::config("remote acknowledged an unexpected protocol"))
}

pub async fn listener_select<S>(stream: &mut S, supported: &[&str]) -> Result<String, RuntimeError>
    where S: AsyncRead + AsyncWrite + Unpin
{
    match listener_select_optional(stream, supported).await? {
        Some(protocol) => Ok(protocol),
        None => Err(RuntimeError::config("requested protocol is not supported by the listener")),
    }
}

pub async fn listener_select_optional<S>(
    stream: &mut S,
    supported: &[&str]
) -> Result<Option<String>, RuntimeError>
    where S: AsyncRead + AsyncWrite + Unpin
{
    debug!(target: TAG, "listener_select: waiting for remote header");
    let remote_header = read_protocol(stream, MAX_PROTOCOL_FRAME_LEN).await?;
    debug!(target: TAG, "listener_select: remote header = {:?}", remote_header);
    if remote_header != MULTISTREAM_V1 {
        return Err(RuntimeError::config("remote did not initiate multistream v1 negotiation"));
    }

    debug!(target: TAG, "listener_select: sending multistream header");
    write_protocol(stream, MULTISTREAM_V1).await?;
    debug!(target: TAG, "listener_select: waiting for protocol request");
    let requested = read_protocol(stream, MAX_PROTOCOL_FRAME_LEN).await?;
    debug!(target: TAG, "listener_select: requested protocol = {:?}, supported = {:?}", requested, supported);

    if supported.iter().any(|candidate| *candidate == requested) {
        debug!(target: TAG, "listener_select: accepting protocol {:?}", requested);
        write_protocol(stream, &requested).await?;
        return Ok(Some(requested));
    }

    debug!(target: TAG, "listener_select: rejecting protocol {:?}", requested);
    write_protocol(stream, MULTISTREAM_NOT_AVAILABLE).await?;
    Ok(None)
}
