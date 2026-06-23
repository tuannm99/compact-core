//! Checkpoint snapshot compression for v0.6.
//!
//! Snapshots are opaque state bytes. The streaming system decides what the
//! bytes mean; compact-core only stores a checked, compressed payload that can
//! be recovered safely.

use crate::{Codec, CompactError, EncodeConfig, Result, Transform, ValueType, checksum32};

const SNAPSHOT_MAGIC: [u8; 4] = *b"SNP1";
const SNAPSHOT_VERSION: u8 = 1;
const FIXED_HEADER_LEN: usize = 4 + 1 + 8 + 8 + 8 + 4;

/// Decoded checkpoint snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub checkpoint_id: u64,
    pub state: Vec<u8>,
}

/// Encode opaque checkpoint state into a checked snapshot payload.
pub fn encode_snapshot(checkpoint_id: u64, state: &[u8]) -> Result<Vec<u8>> {
    let config = snapshot_config();
    let frame = crate::encode_bytes_frame(&config, state)?;
    let frame_len = u64::try_from(frame.len())
        .map_err(|_| CompactError::InvalidInput("snapshot frame is too large"))?;
    let raw_len = u64::try_from(state.len())
        .map_err(|_| CompactError::InvalidInput("snapshot state is too large"))?;
    let checksum = checksum32(&frame);

    let mut out = Vec::with_capacity(FIXED_HEADER_LEN + frame.len());
    out.extend_from_slice(&SNAPSHOT_MAGIC);
    out.push(SNAPSHOT_VERSION);
    out.extend_from_slice(&checkpoint_id.to_le_bytes());
    out.extend_from_slice(&raw_len.to_le_bytes());
    out.extend_from_slice(&frame_len.to_le_bytes());
    out.extend_from_slice(&checksum.to_le_bytes());
    out.extend_from_slice(&frame);

    Ok(out)
}

/// Decode and validate a checkpoint snapshot.
pub fn decode_snapshot(data: &[u8]) -> Result<Snapshot> {
    if data.len() < FIXED_HEADER_LEN {
        return Err(CompactError::InvalidInput("snapshot header is truncated"));
    }

    if data[..4] != SNAPSHOT_MAGIC {
        return Err(CompactError::InvalidInput("snapshot has invalid magic"));
    }

    if data[4] != SNAPSHOT_VERSION {
        return Err(CompactError::Unsupported("snapshot version"));
    }

    let mut cursor = 5usize;
    let checkpoint_id = read_u64(data, &mut cursor, "snapshot checkpoint id is truncated")?;
    let raw_len = read_u64(data, &mut cursor, "snapshot raw length is truncated")?;
    let frame_len = read_u64(data, &mut cursor, "snapshot frame length is truncated")?;
    let expected_checksum = read_u32(data, &mut cursor, "snapshot checksum is truncated")?;
    let frame_len = usize::try_from(frame_len)
        .map_err(|_| CompactError::InvalidInput("snapshot frame length is too large"))?;
    let end = cursor
        .checked_add(frame_len)
        .ok_or(CompactError::InvalidInput("snapshot frame length overflow"))?;
    let frame = data
        .get(cursor..end)
        .ok_or(CompactError::InvalidInput("snapshot frame is truncated"))?;

    if end != data.len() {
        return Err(CompactError::InvalidInput("snapshot has trailing bytes"));
    }

    if checksum32(frame) != expected_checksum {
        return Err(CompactError::InvalidInput("snapshot checksum mismatch"));
    }

    let state = crate::decode_bytes_frame(&snapshot_config(), frame)?;
    if state.len() as u64 != raw_len {
        return Err(CompactError::InvalidInput(
            "snapshot raw length does not match decoded state",
        ));
    }

    Ok(Snapshot {
        checkpoint_id,
        state,
    })
}

fn snapshot_config() -> EncodeConfig {
    EncodeConfig {
        value_type: ValueType::RawBytes,
        transform: Transform::None,
        codec: Codec::Rle,
    }
}

fn read_u64(data: &[u8], cursor: &mut usize, err: &'static str) -> Result<u64> {
    let bytes = read_exact(data, cursor, 8, err)?;

    Ok(u64::from_le_bytes(
        bytes
            .try_into()
            .expect("read_exact returned exactly eight bytes"),
    ))
}

fn read_u32(data: &[u8], cursor: &mut usize, err: &'static str) -> Result<u32> {
    let bytes = read_exact(data, cursor, 4, err)?;

    Ok(u32::from_le_bytes(
        bytes
            .try_into()
            .expect("read_exact returned exactly four bytes"),
    ))
}

fn read_exact<'a>(
    data: &'a [u8],
    cursor: &mut usize,
    len: usize,
    err: &'static str,
) -> Result<&'a [u8]> {
    let end = cursor
        .checked_add(len)
        .ok_or(CompactError::InvalidInput("snapshot length overflow"))?;
    let bytes = data
        .get(*cursor..end)
        .ok_or(CompactError::InvalidInput(err))?;
    *cursor = end;

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::{decode_snapshot, encode_snapshot};
    use crate::CompactError;

    #[test]
    fn snapshot_roundtrips_state() {
        let encoded = encode_snapshot(7, b"aaaaaaaaabbbbbbbbbcccc").unwrap();
        let decoded = decode_snapshot(&encoded).unwrap();

        assert_eq!(decoded.checkpoint_id, 7);
        assert_eq!(decoded.state, b"aaaaaaaaabbbbbbbbbcccc");
    }

    #[test]
    fn snapshot_rejects_corruption() {
        let mut encoded = encode_snapshot(7, b"state").unwrap();
        let last = encoded.len() - 1;
        encoded[last] ^= 0xff;
        let err = decode_snapshot(&encoded).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("snapshot checksum mismatch")
        ));
    }
}
