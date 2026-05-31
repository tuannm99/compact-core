//! Versioned binary frame format.
//!
//! Frames are the storage envelope around codec payloads. A frame answers:
//! which codec produced this payload, how many payload bytes exist, and whether
//! those bytes still match their checksum.

use crate::primitives::crc32;
use crate::{Codec, CompactError, MAGIC_V1, VERSION_V1};

const CODEC_RLE: u8 = 0x01;
const CODEC_DELTA_VARINT_U64: u8 = 0x02;
const CODEC_HUFFMAN: u8 = 0x03;
const CODEC_LZ77: u8 = 0x04;

const MAGIC_LEN: usize = 4;
const VERSION_LEN: usize = 1;
const CODEC_LEN: usize = 1;
const PAYLOAD_LEN_LEN: usize = 8;
const CHECKSUM_LEN: usize = 4;
const HEADER_LEN: usize = MAGIC_LEN + VERSION_LEN + CODEC_LEN + PAYLOAD_LEN_LEN + CHECKSUM_LEN;

/// Parsed frame containing codec metadata and the compressed payload bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub codec: Codec,
    pub payload: Vec<u8>,
}

/// Encode a codec payload into a v1 frame.
///
/// Layout:
/// `[magic:4][version:1][codec:1][payload_len:8 little-endian][crc32:4 little-endian][payload]`
pub fn encode_v1(codec: Codec, payload: &[u8]) -> Vec<u8> {
    let payload_len = payload.len() as u64;
    let checksum = crc32::checksum(payload);

    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(&MAGIC_V1);
    out.push(VERSION_V1);
    out.push(codec_to_id(codec));
    out.extend_from_slice(&payload_len.to_le_bytes());
    out.extend_from_slice(&checksum.to_le_bytes());
    out.extend_from_slice(payload);

    out
}

/// Decode and validate a v1 frame.
///
/// The decoder does not trust length or checksum fields. It validates the
/// header first, checks the payload length against the actual byte slice, then
/// verifies CRC32 before returning payload bytes to the codec layer.
pub fn decode_v1(data: &[u8]) -> Result<Frame, CompactError> {
    if data.len() < HEADER_LEN {
        return Err(CompactError::InvalidInput("frame header is truncated"));
    }

    if data[..MAGIC_LEN] != MAGIC_V1 {
        return Err(CompactError::InvalidInput("invalid frame magic"));
    }

    let version = data[MAGIC_LEN];
    if version != VERSION_V1 {
        return Err(CompactError::Unsupported("frame version"));
    }

    let codec = id_to_codec(data[MAGIC_LEN + VERSION_LEN])?;

    let payload_len_start = MAGIC_LEN + VERSION_LEN + CODEC_LEN;
    let payload_len_end = payload_len_start + PAYLOAD_LEN_LEN;
    let payload_len = u64::from_le_bytes(
        data[payload_len_start..payload_len_end]
            .try_into()
            .expect("slice length is checked by fixed offsets"),
    );
    let payload_len = usize::try_from(payload_len)
        .map_err(|_| CompactError::InvalidInput("frame payload length is too large"))?;

    let checksum_start = payload_len_end;
    let checksum_end = checksum_start + CHECKSUM_LEN;
    let expected_checksum = u32::from_le_bytes(
        data[checksum_start..checksum_end]
            .try_into()
            .expect("slice length is checked by fixed offsets"),
    );

    let payload_start = HEADER_LEN;
    let payload_end = payload_start
        .checked_add(payload_len)
        .ok_or(CompactError::InvalidInput("frame payload length overflow"))?;

    if data.len() != payload_end {
        return Err(CompactError::InvalidInput(
            "frame payload length does not match input",
        ));
    }

    let payload = &data[payload_start..payload_end];
    if !crc32::verify(payload, expected_checksum) {
        return Err(CompactError::InvalidInput("frame checksum mismatch"));
    }

    Ok(Frame {
        codec,
        payload: payload.to_vec(),
    })
}

fn codec_to_id(codec: Codec) -> u8 {
    match codec {
        Codec::Rle => CODEC_RLE,
        Codec::DeltaVarintU64 => CODEC_DELTA_VARINT_U64,
        Codec::Huffman => CODEC_HUFFMAN,
        Codec::Lz77 => CODEC_LZ77,
    }
}

fn id_to_codec(id: u8) -> Result<Codec, CompactError> {
    match id {
        CODEC_RLE => Ok(Codec::Rle),
        CODEC_DELTA_VARINT_U64 => Ok(Codec::DeltaVarintU64),
        CODEC_HUFFMAN => Ok(Codec::Huffman),
        CODEC_LZ77 => Ok(Codec::Lz77),
        _ => Err(CompactError::Unsupported("codec id")),
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_v1, encode_v1};
    use crate::{Codec, CompactError, MAGIC_V1, VERSION_V1};

    #[test]
    fn frame_v1_roundtrip_empty_payload() {
        let encoded = encode_v1(Codec::Rle, b"");
        let decoded = decode_v1(&encoded).unwrap();

        assert_eq!(decoded.codec, Codec::Rle);
        assert_eq!(decoded.payload, b"");
    }

    #[test]
    fn frame_v1_roundtrip_payload() {
        let payload = vec![3, b'A', 3, b'B', 3, b'C'];
        let encoded = encode_v1(Codec::Rle, &payload);
        let decoded = decode_v1(&encoded).unwrap();

        assert_eq!(decoded.codec, Codec::Rle);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn frame_v1_writes_stable_header_fields() {
        let encoded = encode_v1(Codec::DeltaVarintU64, &[100, 1, 1, 28]);

        assert_eq!(&encoded[0..4], &MAGIC_V1);
        assert_eq!(encoded[4], VERSION_V1);
        assert_eq!(encoded[5], 0x02);
        assert_eq!(u64::from_le_bytes(encoded[6..14].try_into().unwrap()), 4);
    }

    #[test]
    fn frame_v1_rejects_truncated_header() {
        let err = decode_v1(&[0; 3]).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("frame header is truncated")
        ));
    }

    #[test]
    fn frame_v1_rejects_invalid_magic() {
        let mut encoded = encode_v1(Codec::Rle, b"payload");
        encoded[0] = b'X';

        let err = decode_v1(&encoded).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("invalid frame magic")
        ));
    }

    #[test]
    fn frame_v1_rejects_unsupported_version() {
        let mut encoded = encode_v1(Codec::Rle, b"payload");
        encoded[4] = VERSION_V1 + 1;

        let err = decode_v1(&encoded).unwrap_err();

        assert!(matches!(err, CompactError::Unsupported("frame version")));
    }

    #[test]
    fn frame_v1_rejects_unknown_codec_id() {
        let mut encoded = encode_v1(Codec::Rle, b"payload");
        encoded[5] = 0xff;

        let err = decode_v1(&encoded).unwrap_err();

        assert!(matches!(err, CompactError::Unsupported("codec id")));
    }

    #[test]
    fn frame_v1_rejects_payload_length_mismatch() {
        let mut encoded = encode_v1(Codec::Rle, b"payload");
        encoded.pop();

        let err = decode_v1(&encoded).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("frame payload length does not match input")
        ));
    }

    #[test]
    fn frame_v1_rejects_checksum_mismatch() {
        let mut encoded = encode_v1(Codec::Rle, b"payload");
        let last = encoded.len() - 1;
        encoded[last] ^= 0xff;

        let err = decode_v1(&encoded).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("frame checksum mismatch")
        ));
    }
}
