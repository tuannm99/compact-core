/// Convert a signed integer into an unsigned integer that keeps small negative
/// numbers small.
///
/// Plainly casting `-1i64` to `u64` produces `u64::MAX`, which is terrible for
/// varint compression. ZigZag instead maps values in this order:
/// `0 -> 0`, `-1 -> 1`, `1 -> 2`, `-2 -> 3`, `2 -> 4`.
pub fn encode_i64(value: i64) -> u64 {
    ((value as u64) << 1) ^ ((value >> 63) as u64)
}

/// Convert a ZigZag-encoded unsigned integer back to the original signed value.
///
/// The low bit stores the sign. Even encoded values are non-negative, odd
/// encoded values are negative.
pub fn decode_u64(value: u64) -> i64 {
    ((value >> 1) as i64) ^ (-((value & 1) as i64))
}

pub fn encode_i64_slice(values: &[i64]) -> Vec<u64> {
    values.iter().copied().map(encode_i64).collect()
}

pub fn decode_u64_slice(values: &[u64]) -> Vec<i64> {
    values.iter().copied().map(decode_u64).collect()
}

#[cfg(test)]
mod tests {
    use super::{decode_u64, decode_u64_slice, encode_i64, encode_i64_slice};

    #[test]
    fn zigzag_known_mappings() {
        let cases = [
            (0, 0),
            (-1, 1),
            (1, 2),
            (-2, 3),
            (2, 4),
            (i64::MAX, u64::MAX - 1),
            (i64::MIN, u64::MAX),
        ];

        for (signed, encoded) in cases {
            assert_eq!(encode_i64(signed), encoded);
            assert_eq!(decode_u64(encoded), signed);
        }
    }

    #[test]
    fn zigzag_slice_roundtrip() {
        let values = [i64::MIN, -100, -2, -1, 0, 1, 2, 100, i64::MAX];
        let encoded = encode_i64_slice(&values);
        let decoded = decode_u64_slice(&encoded);

        assert_eq!(decoded, values);
    }
}
