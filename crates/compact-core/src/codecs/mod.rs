//! Public codec dispatch helpers.
//!
//! Codecs are the first user-facing layer above primitives and pipelines. They
//! still do not write frame headers or checksums; that belongs to `framing`.

use crate::primitives::rle;
use crate::{Codec, CompactError, EncodeConfig, Transform, ValueType};

/// Encode raw bytes with an implemented byte codec.
pub fn encode_bytes(config: &EncodeConfig, input: &[u8]) -> Result<Vec<u8>, CompactError> {
    config.validate()?;

    match (config.value_type, config.transform, config.codec) {
        (ValueType::RawBytes, Transform::None, Codec::Rle) => Ok(rle::encode_rle(input)),
        _ => Err(CompactError::Unsupported("byte codec configuration")),
    }
}

/// Decode raw bytes with an implemented byte codec.
pub fn decode_bytes(config: &EncodeConfig, input: &[u8]) -> Result<Vec<u8>, CompactError> {
    config.validate()?;

    match (config.value_type, config.transform, config.codec) {
        (ValueType::RawBytes, Transform::None, Codec::Rle) => rle::decode_rle(input),
        _ => Err(CompactError::Unsupported("byte codec configuration")),
    }
}

/// Encode `u64` values with an implemented numeric codec.
pub fn encode_u64(config: &EncodeConfig, input: &[u64]) -> Result<Vec<u8>, CompactError> {
    config.validate()?;

    match (config.value_type, config.transform, config.codec) {
        (ValueType::U64, Transform::Delta, Codec::DeltaVarintU64) => {
            crate::pipeline::delta_varint::encode_u64(input)
        }
        _ => Err(CompactError::Unsupported("u64 codec configuration")),
    }
}

/// Decode `u64` values with an implemented numeric codec.
pub fn decode_u64(config: &EncodeConfig, input: &[u8]) -> Result<Vec<u64>, CompactError> {
    config.validate()?;

    match (config.value_type, config.transform, config.codec) {
        (ValueType::U64, Transform::Delta, Codec::DeltaVarintU64) => {
            crate::pipeline::delta_varint::decode_u64(input)
        }
        _ => Err(CompactError::Unsupported("u64 codec configuration")),
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_bytes, decode_u64, encode_bytes, encode_u64};
    use crate::{Codec, CompactError, EncodeConfig, Transform, ValueType};

    #[test]
    fn byte_codec_rle_roundtrip() {
        let config = EncodeConfig {
            value_type: ValueType::RawBytes,
            transform: Transform::None,
            codec: Codec::Rle,
        };

        let input = b"AAABBBCCC";
        let encoded = encode_bytes(&config, input).unwrap();
        let decoded = decode_bytes(&config, &encoded).unwrap();

        assert_eq!(encoded, vec![3, b'A', 3, b'B', 3, b'C']);
        assert_eq!(decoded, input);
    }

    #[test]
    fn u64_codec_delta_varint_roundtrip() {
        let config = EncodeConfig {
            value_type: ValueType::U64,
            transform: Transform::Delta,
            codec: Codec::DeltaVarintU64,
        };

        let input = [100, 101, 102, 130];
        let encoded = encode_u64(&config, &input).unwrap();
        let decoded = decode_u64(&config, &encoded).unwrap();

        assert_eq!(encoded, vec![100, 1, 1, 28]);
        assert_eq!(decoded, input);
    }

    #[test]
    fn codec_validation_rejects_unsupported_future_codecs() {
        let config = EncodeConfig {
            value_type: ValueType::RawBytes,
            transform: Transform::None,
            codec: Codec::Huffman,
        };

        let err = encode_bytes(&config, b"data").unwrap_err();

        assert!(matches!(err, CompactError::Unsupported("huffman codec")));
    }

    #[test]
    fn byte_codec_rejects_numeric_config() {
        let config = EncodeConfig {
            value_type: ValueType::U64,
            transform: Transform::Delta,
            codec: Codec::DeltaVarintU64,
        };

        let err = encode_bytes(&config, b"data").unwrap_err();

        assert!(matches!(
            err,
            CompactError::Unsupported("byte codec configuration")
        ));
    }
}
