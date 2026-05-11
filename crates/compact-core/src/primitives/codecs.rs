use std::io;

use crate::CompactError;

///
/// Layouts: [count][byte][count][byte]...
///
/// AAABBBCCC -> 3A 3B 3C
/// In case more than 255 (u8 max) then split it -> 255A 45A 3B 3C
pub fn encode_rle(data: &[u8]) -> Vec<u8> {
    // worse case is no char can compress, x2 spaces
    let mut result = Vec::with_capacity(data.len() * 2);

    let mut i = 0;
    while i < data.len() {
        let byte = data[i];
        let mut count = 1;

        while i + count < data.len() && data[i + count] == byte && count < u8::MAX as usize {
            count += 1;
        }

        result.push(count as u8);
        result.push(byte);

        i += count;
    }

    result
}

pub fn decode_rle(data: &[u8]) -> Result<Vec<u8>, CompactError> {
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

        estimated_size += count;
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
    use crate::primitives::codecs::{decode_rle, encode_rle};

    #[test]
    fn encode_rle_empty() {
        assert_eq!(encode_rle(b""), vec![]);
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
        assert_eq!(decode_rle(b"").unwrap(), vec![]);
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
