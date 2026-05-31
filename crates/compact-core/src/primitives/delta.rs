use crate::CompactError;

/// Delta encoding stores the first value unchanged, then stores each later
/// value as the difference from the previous original value.
///
/// This module works on typed numeric values. That is important: delta encoding
/// is a model for columns such as timestamps, counters, IDs, or metrics. It is
/// not a good generic transform for arbitrary file bytes.
///
/// Encode monotonically non-decreasing `u64` values.
///
/// The output has the same number of values as the input:
/// `[base, delta_1, delta_2, ...]`.
///
/// Decreasing input is rejected because an unsigned delta cannot represent a
/// negative difference. Use `encode_delta_i64` when negative deltas are valid.
pub fn encode_delta_u64(values: &[u64]) -> Result<Vec<u64>, CompactError> {
    if values.is_empty() {
        return Ok(Vec::new());
    }

    let mut out = Vec::with_capacity(values.len());
    out.push(values[0]);

    for window in values.windows(2) {
        let previous = window[0];
        let current = window[1];
        let delta = current
            .checked_sub(previous)
            .ok_or(CompactError::InvalidInput("u64 delta cannot be negative"))?;

        out.push(delta);
    }

    Ok(out)
}

/// Rebuild original `u64` values from `[base, delta_1, delta_2, ...]`.
///
/// Overflow is rejected because accepting it would silently wrap the decoded
/// series and corrupt the column.
pub fn decode_delta_u64(values: &[u64]) -> Result<Vec<u64>, CompactError> {
    if values.is_empty() {
        return Ok(Vec::new());
    }

    let mut out = Vec::with_capacity(values.len());
    let mut previous = values[0];
    out.push(previous);

    for &delta in &values[1..] {
        let current = previous
            .checked_add(delta)
            .ok_or(CompactError::InvalidInput("u64 delta decode overflow"))?;

        out.push(current);
        previous = current;
    }

    Ok(out)
}

/// Encode signed `i64` values where adjacent differences can be positive,
/// negative, or zero.
///
/// The output shape is `[base, delta_1, delta_2, ...]`. Signed deltas are often
/// followed by ZigZag and Varint encoding so small negative changes still use a
/// small number of bytes.
pub fn encode_delta_i64(values: &[i64]) -> Result<Vec<i64>, CompactError> {
    if values.is_empty() {
        return Ok(Vec::new());
    }

    let mut out = Vec::with_capacity(values.len());
    out.push(values[0]);

    for window in values.windows(2) {
        let previous = window[0];
        let current = window[1];
        let delta = current
            .checked_sub(previous)
            .ok_or(CompactError::InvalidInput("i64 delta encode overflow"))?;

        out.push(delta);
    }

    Ok(out)
}

/// Rebuild original `i64` values from `[base, delta_1, delta_2, ...]`.
pub fn decode_delta_i64(values: &[i64]) -> Result<Vec<i64>, CompactError> {
    if values.is_empty() {
        return Ok(Vec::new());
    }

    let mut out = Vec::with_capacity(values.len());
    let mut previous = values[0];
    out.push(previous);

    for &delta in &values[1..] {
        let current = previous
            .checked_add(delta)
            .ok_or(CompactError::InvalidInput("i64 delta decode overflow"))?;

        out.push(current);
        previous = current;
    }

    Ok(out)
}

/// Byte-level delta transform kept for low-level experiments and byte tests.
///
/// This uses wrapping arithmetic because one byte can only hold `0..=255`.
/// For production column compression prefer the typed `u64` or `i64` helpers
/// above, then serialize those values with Varint/ZigZag as needed.
pub fn encode_delta(data: &[u8]) -> Vec<u8> {
    if data.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::with_capacity(data.len());
    result.push(data[0]);

    for i in 1..data.len() {
        result.push(data[i].wrapping_sub(data[i - 1]));
    }

    result
}

/// Reverse `encode_delta` for byte slices.
pub fn decode_delta(data: &[u8]) -> Vec<u8> {
    if data.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::with_capacity(data.len());
    result.push(data[0]);

    for i in 1..data.len() {
        let previous = result[i - 1];
        result.push(previous.wrapping_add(data[i]));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::{
        decode_delta, decode_delta_i64, decode_delta_u64, encode_delta, encode_delta_i64,
        encode_delta_u64,
    };
    use crate::CompactError;

    #[test]
    fn delta_u64_empty() {
        assert_eq!(encode_delta_u64(&[]).unwrap(), Vec::<u64>::new());
        assert_eq!(decode_delta_u64(&[]).unwrap(), Vec::<u64>::new());
    }

    #[test]
    fn delta_u64_encodes_monotonic_values() {
        let values = [100, 100, 101, 120, 120, 200];

        assert_eq!(
            encode_delta_u64(&values).unwrap(),
            vec![100, 0, 1, 19, 0, 80]
        );
    }

    #[test]
    fn delta_u64_roundtrip() {
        let cases: &[&[u64]] = &[
            &[],
            &[0],
            &[1, 2, 3, 4],
            &[1_710_000_000, 1_710_000_001, 1_710_000_010],
            &[u64::MAX - 2, u64::MAX - 1, u64::MAX],
        ];

        for &case in cases {
            let encoded = encode_delta_u64(case).unwrap();
            let decoded = decode_delta_u64(&encoded).unwrap();

            assert_eq!(decoded, case);
        }
    }

    #[test]
    fn delta_u64_rejects_decreasing_input() {
        let err = encode_delta_u64(&[10, 9]).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("u64 delta cannot be negative")
        ));
    }

    #[test]
    fn delta_u64_rejects_decode_overflow() {
        let err = decode_delta_u64(&[u64::MAX, 1]).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("u64 delta decode overflow")
        ));
    }

    #[test]
    fn delta_i64_roundtrip_with_negative_deltas() {
        let values = [100, 98, 101, -20, -20, i64::MIN + 2];
        let encoded = encode_delta_i64(&values).unwrap();
        let decoded = decode_delta_i64(&encoded).unwrap();

        assert_eq!(encoded, vec![100, -2, 3, -121, 0, i64::MIN + 22]);
        assert_eq!(decoded, values);
    }

    #[test]
    fn delta_i64_rejects_encode_overflow() {
        let err = encode_delta_i64(&[i64::MAX, i64::MIN]).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("i64 delta encode overflow")
        ));
    }

    #[test]
    fn delta_i64_rejects_decode_overflow() {
        let err = decode_delta_i64(&[i64::MAX, 1]).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("i64 delta decode overflow")
        ));
    }

    #[test]
    fn byte_delta_known_values() {
        assert_eq!(encode_delta(&[10, 12, 9, 9]), vec![10, 2, 253, 0]);
        assert_eq!(decode_delta(&[10, 2, 253, 0]), vec![10, 12, 9, 9]);
    }

    #[test]
    fn byte_delta_roundtrip() {
        let cases: &[&[u8]] = &[
            b"",
            b"A",
            b"AAABBBCCC",
            b"ABC",
            b"hello world",
            &[0, 255, 0, 128],
        ];

        for &case in cases {
            let encoded = encode_delta(case);
            let decoded = decode_delta(&encoded);

            assert_eq!(decoded, case);
        }
    }
}
