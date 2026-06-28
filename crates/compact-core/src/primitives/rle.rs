use crate::CompactError;
use crate::limits::MAX_DECODED_BYTES;

/// Encode bytes with run-length encoding.
///
/// The wire layout is repeated `[count][byte]` pairs. `count` is a `u8`, so a
/// run can store at most 255 copies of the same byte. Longer runs are split
/// into multiple pairs:
///
/// `300 * b'A' -> [255, b'A', 45, b'A']`
pub fn encode_rle(data: &[u8]) -> Vec<u8> {
    // Worst case is no repeated bytes: every input byte becomes `[1][byte]`.
    let mut result = Vec::with_capacity(data.len() * 2);

    let mut i = 0;
    while i < data.len() {
        let byte = data[i];
        let mut count = 1;

        // Extend this run while the next byte matches and the one-byte count
        // can still represent the run length.
        while i + count < data.len() && data[i + count] == byte && count < u8::MAX as usize {
            count += 1;
        }

        result.push(count as u8);
        result.push(byte);

        i += count;
    }

    result
}

/// Decode `[count][byte]` RLE pairs back into the original byte stream.
///
/// Malformed input returns an error instead of guessing:
/// - odd-length input cannot be split into complete pairs
/// - count zero is invalid because the encoder never emits empty runs
pub fn decode_rle(data: &[u8]) -> Result<Vec<u8>, CompactError> {
    decode_rle_bounded(data, MAX_DECODED_BYTES)
}

/// Decode RLE pairs while rejecting output larger than `max_output_len`.
///
/// Callers that have a tighter size from enclosing metadata should use this
/// function instead of relying on the crate-wide defensive limit.
pub fn decode_rle_bounded(data: &[u8], max_output_len: usize) -> Result<Vec<u8>, CompactError> {
    if !data.len().is_multiple_of(2) {
        return Err(CompactError::InvalidInput(
            "RLE data must have even length (count, value) pairs",
        ));
    }

    let mut estimated_size = 0usize;

    for chunk in data.chunks_exact(2) {
        let count = chunk[0] as usize;

        if count == 0 {
            return Err(CompactError::InvalidInput("RLE run count of 0 is invalid"));
        }

        estimated_size = estimated_size
            .checked_add(count)
            .ok_or(CompactError::InvalidInput("RLE decoded length overflow"))?;
        if estimated_size > max_output_len {
            return Err(CompactError::InvalidInput(
                "RLE decoded length exceeds configured limit",
            ));
        }
    }

    let mut result = Vec::with_capacity(estimated_size);

    for chunk in data.chunks_exact(2) {
        let count = chunk[0] as usize;
        let value = chunk[1];

        result.extend(std::iter::repeat_n(value, count));
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use crate::CompactError;
    use crate::primitives::rle::{decode_rle, decode_rle_bounded, encode_rle};

    #[test]
    fn encode_rle_empty() {
        assert_eq!(encode_rle(b""), Vec::<u8>::new());
    }

    #[test]
    fn encode_rle_basic() {
        assert_eq!(encode_rle(b"AAABBBCCC"), vec![3, b'A', 3, b'B', 3, b'C']);
    }

    #[test]

    fn encode_rle_no_compression() {
        assert_eq!(encode_rle(b"ABC"), vec![1, b'A', 1, b'B', 1, b'C']);
    }

    #[test]
    fn encode_rle_more_than_255() {
        let data = vec![b'A'; 300];
        let encoded = encode_rle(&data);

        assert_eq!(encoded, vec![255, b'A', 45, b'A']);
    }

    #[test]
    fn decode_rle_empty() {
        assert_eq!(decode_rle(b"").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn decode_rle_basic() {
        let encoded = vec![3, b'A', 3, b'B', 3, b'C'];
        let decoded = decode_rle(&encoded).unwrap();

        assert_eq!(decoded, b"AAABBBCCC");
    }

    #[test]
    fn decode_rle_more_than_255() {
        let encoded = vec![255, b'A', 45, b'A'];
        let decoded = decode_rle(&encoded).unwrap();

        assert_eq!(decoded, vec![b'A'; 300]);
    }

    #[test]
    fn decode_rle_rejects_odd_length_input() {
        let err = decode_rle(&[3, b'A', 2]).unwrap_err();

        assert!(matches!(err, CompactError::InvalidInput(_)));
    }

    #[test]
    fn decode_rle_rejects_zero_count() {
        let err = decode_rle(&[0, b'A']).unwrap_err();

        assert!(matches!(err, CompactError::InvalidInput(_)));
    }

    #[test]
    fn decode_rle_rejects_output_above_caller_limit() {
        let err = decode_rle_bounded(&[5, b'A'], 4).unwrap_err();

        assert!(matches!(err, CompactError::InvalidInput(_)));
    }

    #[test]
    fn rle_roundtrip() {
        let cases: Vec<&[u8]> = vec![
            b"",
            b"A",
            b"AAABBBCCC",
            b"ABC",
            b"AAAAABCCDDDD",
            b"hello world",
        ];

        for case in cases {
            let encoded = encode_rle(case);
            let decoded = decode_rle(&encoded).unwrap();

            assert_eq!(decoded, case);
        }
    }

    #[test]
    fn rle_roundtrip_more_than_255() {
        let data = vec![b'A'; 300];

        let encoded = encode_rle(&data);
        let decoded = decode_rle(&encoded).unwrap();

        assert_eq!(decoded, data);
    }
}
