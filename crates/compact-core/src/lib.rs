use std::io as std_io;

use thiserror::Error;

pub mod codecs;
pub mod format;
pub mod framing;
pub mod io;
pub mod pipeline;
pub mod primitives;
pub mod schema;
pub mod statistics;
pub mod streaming;
mod transforms;

pub const MAGIC_V1: [u8; 4] = *b"CMP1";
pub const VERSION_V1: u8 = 1;
pub const MAGIC_V2: [u8; 4] = *b"CMP2";
pub const VERSION_V2: u8 = 2;
pub const MAGIC_V3: [u8; 4] = *b"CMP3";
pub const VERSION_V3: u8 = 3;

pub type Result<T> = std::result::Result<T, CompactError>;

#[derive(Debug, Error)]
pub enum CompactError {
    #[error("invalid input: {0}")]
    InvalidInput(&'static str),
    #[error("unsupported feature: {0}")]
    Unsupported(&'static str),
    #[error("i/o error: {0}")]
    Io(#[from] std_io::Error),
}

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn checksum32(input: &[u8]) -> u32 {
    primitives::crc32::checksum(input)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    RawBytes,
    String,
    U32,
    U64,
    I32,
    I64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transform {
    None,
    Delta,
    ZigZag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    Rle,
    DeltaVarintU64,
    ColumnBlock,
    Huffman,
    Lz77,
}

pub struct EncodeConfig {
    pub value_type: ValueType,
    pub transform: Transform,
    pub codec: Codec,
}

impl EncodeConfig {
    /// Validate whether this config is implemented by the current core crate.
    ///
    /// The enum contains future roadmap codecs so callers can model intent, but
    /// execution paths should call this before starting work and fail loudly for
    /// combinations that do not exist yet.
    pub fn validate(&self) -> Result<()> {
        match (self.value_type, self.transform, self.codec) {
            (ValueType::RawBytes, Transform::None, Codec::Rle) => Ok(()),
            (ValueType::String, Transform::None, Codec::Rle) => Ok(()),
            (ValueType::U64, Transform::Delta, Codec::DeltaVarintU64) => Ok(()),
            (_, _, Codec::ColumnBlock) => Ok(()),
            (_, _, Codec::Huffman) => Err(CompactError::Unsupported("huffman codec")),
            (_, _, Codec::Lz77) => Err(CompactError::Unsupported("lz77 codec")),
            _ => Err(CompactError::Unsupported("codec configuration")),
        }
    }
}

/// Encode raw bytes with the configured byte codec and wrap the result in a v1 frame.
pub fn encode_bytes_frame(config: &EncodeConfig, input: &[u8]) -> Result<Vec<u8>> {
    config.validate()?;

    let payload = codecs::encode_bytes(config, input)?;

    Ok(framing::encode_v1(config.codec, &payload))
}

/// Decode a v1 frame, validate its codec against `config`, then decode raw bytes.
pub fn decode_bytes_frame(config: &EncodeConfig, frame: &[u8]) -> Result<Vec<u8>> {
    config.validate()?;

    let frame = framing::decode_v1(frame)?;
    ensure_frame_codec(config, frame.codec)?;

    codecs::decode_bytes(config, &frame.payload)
}

/// Encode `u64` values with the configured numeric codec and wrap the result in a v1 frame.
pub fn encode_u64_frame(config: &EncodeConfig, values: &[u64]) -> Result<Vec<u8>> {
    config.validate()?;

    let payload = codecs::encode_u64(config, values)?;

    Ok(framing::encode_v1(config.codec, &payload))
}

/// Decode a v1 frame, validate its codec against `config`, then decode `u64` values.
pub fn decode_u64_frame(config: &EncodeConfig, frame: &[u8]) -> Result<Vec<u64>> {
    config.validate()?;

    let frame = framing::decode_v1(frame)?;
    ensure_frame_codec(config, frame.codec)?;

    codecs::decode_u64(config, &frame.payload)
}

fn ensure_frame_codec(config: &EncodeConfig, actual: Codec) -> Result<()> {
    if actual != config.codec {
        return Err(CompactError::InvalidInput(
            "frame codec does not match requested config",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        Codec, CompactError, EncodeConfig, MAGIC_V1, Transform, VERSION_V1, ValueType, checksum32,
        crate_version, decode_bytes_frame, decode_u64_frame, encode_bytes_frame, encode_u64_frame,
    };

    #[test]
    fn version_is_set() {
        assert_eq!(crate_version(), "0.3.0");
    }

    #[test]
    fn frame_constants_match_v1() {
        assert_eq!(MAGIC_V1, *b"CMP1");
        assert_eq!(VERSION_V1, 1);
    }

    #[test]
    fn format_constants_match_v3() {
        assert_eq!(crate::MAGIC_V3, *b"CMP3");
        assert_eq!(crate::VERSION_V3, 3);
    }

    #[test]
    fn checksum_is_stable() {
        assert_eq!(checksum32(b"compact"), 0x84_a0_0b_bf);
    }

    #[test]
    fn bytes_frame_rle_roundtrip() {
        let config = EncodeConfig {
            value_type: ValueType::RawBytes,
            transform: Transform::None,
            codec: Codec::Rle,
        };
        let encoded = encode_bytes_frame(&config, b"AAABBBCCC").unwrap();
        let decoded = decode_bytes_frame(&config, &encoded).unwrap();

        assert_eq!(decoded, b"AAABBBCCC");
    }

    #[test]
    fn u64_frame_delta_varint_roundtrip() {
        let config = EncodeConfig {
            value_type: ValueType::U64,
            transform: Transform::Delta,
            codec: Codec::DeltaVarintU64,
        };
        let values = [100, 101, 102, 130];
        let encoded = encode_u64_frame(&config, &values).unwrap();
        let decoded = decode_u64_frame(&config, &encoded).unwrap();

        assert_eq!(decoded, values);
    }

    #[test]
    fn frame_decode_rejects_codec_config_mismatch() {
        let encode_config = EncodeConfig {
            value_type: ValueType::RawBytes,
            transform: Transform::None,
            codec: Codec::Rle,
        };
        let decode_config = EncodeConfig {
            value_type: ValueType::U64,
            transform: Transform::Delta,
            codec: Codec::DeltaVarintU64,
        };
        let encoded = encode_bytes_frame(&encode_config, b"AAABBBCCC").unwrap();
        let err = decode_u64_frame(&decode_config, &encoded).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("frame codec does not match requested config")
        ));
    }

    #[test]
    fn bytes_frame_rejects_corrupted_payload() {
        let config = EncodeConfig {
            value_type: ValueType::RawBytes,
            transform: Transform::None,
            codec: Codec::Rle,
        };
        let mut encoded = encode_bytes_frame(&config, b"AAABBBCCC").unwrap();
        let last = encoded.len() - 1;
        encoded[last] ^= 0xff;
        let err = decode_bytes_frame(&config, &encoded).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("frame checksum mismatch")
        ));
    }
}
