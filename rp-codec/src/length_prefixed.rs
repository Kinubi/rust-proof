use std::io::{self, ErrorKind};

use futures::io::{AsyncRead, AsyncReadExt};

pub fn encode_length_prefixed(payload: &[u8], max_len: u32) -> io::Result<Vec<u8>> {
    if payload.len() > (max_len as usize) {
        return Err(
            io::Error::new(ErrorKind::InvalidData, "payload exceeds configured frame length limit")
        );
    }

    let mut buffer = unsigned_varint::encode::u32_buffer();
    let prefix = unsigned_varint::encode::u32(payload.len() as u32, &mut buffer);
    let mut framed = Vec::with_capacity(prefix.len() + payload.len());
    framed.extend_from_slice(prefix);
    framed.extend_from_slice(payload);
    Ok(framed)
}

pub fn decode_length_prefix(input: &[u8], max_len: u32) -> io::Result<(usize, &[u8])> {
    let (payload_len, remaining) = unsigned_varint::decode::u32(input).map_err(|error| {
        io::Error::new(ErrorKind::InvalidData, format!("invalid length prefix: {error}"))
    })?;

    if payload_len > max_len {
        return Err(
            io::Error::new(
                ErrorKind::InvalidData,
                "length prefix exceeds configured frame length limit"
            )
        );
    }

    let header_len = input.len() - remaining.len();
    let total_len = header_len + (payload_len as usize);
    if input.len() < total_len {
        return Err(
            io::Error::new(ErrorKind::UnexpectedEof, "frame body shorter than advertised length")
        );
    }

    Ok((header_len, &input[header_len..total_len]))
}

pub fn decode_length_prefixed_payload_len(prefix: &[u8], max_len: u32) -> io::Result<usize> {
    let (payload_len, _) = unsigned_varint::decode::u32(prefix).map_err(|error| {
        io::Error::new(
            ErrorKind::InvalidData,
            format!("invalid frame length prefix: {error}")
        )
    })?;

    if payload_len > max_len {
        return Err(
            io::Error::new(ErrorKind::InvalidData, "frame length exceeds configured maximum")
        );
    }

    Ok(payload_len as usize)
}

pub async fn read_length_prefixed_frame<S>(stream: &mut S, max_len: u32) -> io::Result<Vec<u8>>
where
    S: AsyncRead + Unpin,
{
    let mut prefix = [0u8; 5];
    let mut prefix_len = 0usize;

    loop {
        if prefix_len >= prefix.len() {
            return Err(
                io::Error::new(
                    ErrorKind::InvalidData,
                    "frame length prefix exceeds u32 varint width"
                )
            );
        }

        stream.read_exact(&mut prefix[prefix_len..=prefix_len]).await?;
        prefix_len += 1;

        if (prefix[prefix_len - 1] & 0x80) == 0 {
            let payload_len = decode_length_prefixed_payload_len(&prefix[..prefix_len], max_len)?;
            let mut frame = Vec::with_capacity(prefix_len + payload_len);
            frame.extend_from_slice(&prefix[..prefix_len]);
            frame.resize(prefix_len + payload_len, 0);
            stream.read_exact(&mut frame[prefix_len..]).await?;
            return Ok(frame);
        }
    }
}