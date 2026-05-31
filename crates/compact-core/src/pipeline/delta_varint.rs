use crate::CompactError;
use crate::primitives::{delta, varint};

/// Encode monotonically non-decreasing `u64` values with delta + varint.
///
/// This is the first useful numeric pipeline:
/// `values -> unsigned deltas -> base-128 varint bytes`.
///
/// It works well for timestamps, sorted IDs, counters, and other series where
/// each value is usually close to the previous value. Decreasing input is
/// rejected by `delta::encode_delta_u64` because unsigned deltas cannot express
/// negative changes.
pub fn encode_u64(values: &[u64]) -> Result<Vec<u8>, CompactError> {
    let deltas = delta::encode_delta_u64(values)?;

    Ok(varint::encode_u64(&deltas))
}

/// Decode bytes produced by `encode_u64` back into the original `u64` values.
///
/// Decode validation happens in two stages. Varint rejects malformed byte
/// streams first, then delta decode rejects arithmetic overflow while rebuilding
/// the original values.
pub fn decode_u64(data: &[u8]) -> Result<Vec<u64>, CompactError> {
    let deltas = varint::decode_u64(data)?;

    delta::decode_delta_u64(&deltas)
}

#[cfg(test)]
mod tests {
    use super::{decode_u64, encode_u64};
    use crate::CompactError;

    #[test]
    fn delta_varint_empty() {
        let encoded = encode_u64(&[]).unwrap();
        let decoded = decode_u64(&encoded).unwrap();

        assert_eq!(encoded, Vec::<u8>::new());
        assert_eq!(decoded, Vec::<u64>::new());
    }

    #[test]
    fn delta_varint_known_encoding_for_small_deltas() {
        let values = [100, 101, 102, 130];
        let encoded = encode_u64(&values).unwrap();

        assert_eq!(encoded, vec![100, 1, 1, 28]);
        assert_eq!(decode_u64(&encoded).unwrap(), values);
    }

    #[test]
    fn delta_varint_roundtrip_representative_series() {
        let cases: &[&[u64]] = &[
            &[0],
            &[0, 0, 0, 0],
            &[1, 2, 3, 4, 5],
            &[1_710_000_000, 1_710_000_001, 1_710_000_010],
            &[u64::MAX - 3, u64::MAX - 2, u64::MAX - 1, u64::MAX],
        ];

        for &case in cases {
            let encoded = encode_u64(case).unwrap();
            let decoded = decode_u64(&encoded).unwrap();

            assert_eq!(decoded, case);
        }
    }

    #[test]
    fn delta_varint_rejects_decreasing_series() {
        let err = encode_u64(&[10, 9]).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("u64 delta cannot be negative")
        ));
    }

    #[test]
    fn delta_varint_rejects_malformed_varint() {
        let err = decode_u64(&[0x80]).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("truncated varint")
        ));
    }

    #[test]
    fn delta_varint_rejects_decode_overflow() {
        let encoded = crate::primitives::varint::encode_u64(&[u64::MAX, 1]);
        let err = decode_u64(&encoded).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("u64 delta decode overflow")
        ));
    }
}
