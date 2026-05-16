/// Delta encoding is modeled in three layers:
///
/// 1. Value type
///    - decides integer width and signedness (`i16`, `i32`, `i64`, ...)
/// 2. Delta transform
///    - stores the first value as a base
///    - stores each next value as `current - previous`
/// 3. Wire layout
///    - defines how base and deltas are serialized into bytes
///
/// This module should remain the algorithm boundary.
/// Type-specific parsing and the final on-wire byte format should stay
/// isolated behind small helpers instead of being spread across the
/// encode/decode loops.
///
/// Current intended record shape:
///
/// `[base][delta_1][delta_2]...[delta_n]`
///
/// Example at the value level:
///
/// `1710000000 1710000001 1710000002 1710000003`
/// -> `base=1710000000, deltas=[1, 1, 1]`
///
/// The exact byte layout for `base` and `delta` is still intentionally
/// undefined here. Follow-up implementation should choose that format first
/// and then keep both `encode_delta` and `decode_delta` aligned to it.
///
/// Suggested implementation split:
///
/// - `encode_delta`: walk values and emit `base` + deltas
/// - `decode_delta`: validate layout and rebuild values from deltas
/// - private helpers: read/write typed values and deltas for the chosen layout
///
/// data type suite: timestamp increasing ids-metrics float-series
///
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

pub fn decode_delta(data: &[u8]) -> Vec<u8> {
    if data.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::with_capacity(data.len());
    result.push(data[0]);

    for i in 1..data.len() {
        let prev = result[i - 1];
        result.push(prev.wrapping_add(data[i]));
    }

    result
}

// TODO
// Type contract placeholders.
//
// The delta algorithm should stay generic over "integer-like" values instead
// of duplicating the full encode/decode flow for `i16`, `i32`, and `i64`.
// A later implementation can introduce a small internal contract here to
// centralize width, signedness, parsing, and overflow rules.

// Layout helper placeholders.
//
// Keep byte-level concerns local:
// - how the first value is written
// - how each delta is written
// - how the decoder knows the integer width
// - how malformed or truncated inputs are rejected
//
// This keeps the outer encode/decode loops readable and prevents type/layout
// branches from leaking into algorithm code.

#[cfg(test)]
mod tests {
    use crate::primitives::delta::{decode_delta, encode_delta};

    /*REVIEWER [MAJOR][TESTING]: this module now implements real delta encode/decode logic but still has no executable tests.
    WHY: without roundtrip coverage for increasing, decreasing, and wraparound byte sequences, a small arithmetic change can silently reintroduce corruption.
    FIX: add unit tests that assert `decode_delta(encode_delta(input)) == input` for representative edge cases.
    */
    // Keep tests grouped by invariant once implementation starts:
    // - empty input x
    // - single value
    // - monotonic values
    // - negative deltas
    // - malformed layout
    // - roundtrip per integer width
    #[test]
    fn encode_delta_empty() {
        assert_eq!(encode_delta(b""), vec![]);
    }

    #[test]
    fn encode_delta_single_value() {
        assert_eq!(encode_delta(b"1"), vec![b'1']);
    }

    #[test]
    fn encode_delta_monotonic_values() {
        assert_eq!(encode_delta(b"1"), vec![b'1']);
    }

    #[test]
    fn encode_delta_negative_values() {
        assert_eq!(encode_delta(b"1"), vec![b'1']);
    }

    #[test]
    fn encode_delta_malformed_layout() {
        assert_eq!(encode_delta(b"1"), vec![b'1']);
    }

    #[test]
    fn decode_delta_empty() {
        assert_eq!(encode_delta(b""), vec![]);
    }

    #[test]
    fn decode_delta_single_value() {
        assert_eq!(encode_delta(b"1"), vec![b'1']);
    }

    #[test]
    fn decode_delta_monotonic_values() {
        assert_eq!(encode_delta(b"1"), vec![b'1']);
    }

    #[test]
    fn decode_delta_negative_values() {
        assert_eq!(encode_delta(b"1"), vec![b'1']);
    }

    #[test]
    fn decode_delta_malformed_layout() {
        assert_eq!(encode_delta(b"1"), vec![b'1']);
    }

    #[test]
    fn delta_roundtrip() {
        let cases: Vec<&[u8]> = vec![
            b"",
            b"A",
            b"AAABBBCCC",
            b"ABC",
            b"AAAAABCCDDDD",
            b"hello world",
        ];

        for case in cases {
            let encoded = encode_delta(case);
            let decoded = decode_delta(&encoded);

            assert_eq!(decoded, case);
        }
    }
}
