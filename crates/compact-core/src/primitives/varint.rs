use crate::CompactError;

pub fn encode_u64(values: &[u64]) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() * 10);

    for &v in values {
        let mut value = v;

        while value >= 0x80 {
            out.push((value as u8 & 0x7f) | 0x80);
            value >>= 7;
        }

        out.push(value as u8);
    }

    out
}

pub fn decode_u64(data: &[u8]) -> Result<Vec<u64>, CompactError> {
    let mut out = Vec::new();

    let mut value = 0u64;
    let mut shift = 0u32;

    for &byte in data {
        let part = (byte & 0x7f) as u64;

        /*REVIEWER [BLOCKER][CORRECTNESS]: the decoder accepts invalid 10-byte sequences whose last payload bits overflow `u64`.
        WHY: after nine continuation bytes `shift == 63`, so a final byte such as `0x02` is silently truncated by `part << shift` instead of being rejected. That decodes malformed input to the wrong integer and breaks wire-format validation.
        FIX: reject any byte once `shift == 63` unless `part <= 1` and the byte is terminal, or switch to `checked_shl`/explicit length validation so overflowing payload bits return `CompactError::InvalidInput(\"varint overflow\")`.
        */
        if shift >= 64 {
            return Err(CompactError::InvalidInput("varint overflow"));
        }

        value |= part << shift;

        if byte & 0x80 == 0 {
            /*REVIEWER [BLOCKER][CORRECTNESS]: the decoder accepts overlong encodings such as `[0x80, 0x00]` for zero.
            WHY: multiple byte sequences then map to the same integer, which makes the wire format non-canonical and defeats any caller that relies on a single stable encoding for hashing, deduplication, or validation.
            FIX: track how many bytes were consumed for the current value and reject any representation that is longer than the minimal varint encoding for the decoded integer.
            */
            out.push(value);
            value = 0;
            shift = 0;
        } else {
            shift += 7;
        }
    }

    if shift != 0 {
        return Err(CompactError::InvalidInput("truncated varint"));
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{decode_u64, encode_u64};
    use crate::CompactError;

    #[test]
    fn encode_u64_edgecase() {
        assert_eq!(encode_u64(&[]), vec![]);
        assert_eq!(encode_u64(&[0]), vec![0x00]);
        assert_eq!(encode_u64(&[1]), vec![0x01]);
        assert_eq!(encode_u64(&[127]), vec![0x7f]);
        assert_eq!(encode_u64(&[128]), vec![0x80, 0x01]);
        assert_eq!(
            encode_u64(&[u64::MAX]),
            vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01]
        );
    }

    #[test]
    fn decode_u64_truncated() {
        let cases: &[&[u8]] = &[&[0x80], &[0x80, 0x80], &[0xac, 0x02, 0x81]];

        for &case in cases {
            let err = decode_u64(case).unwrap_err();
            assert!(matches!(
                err,
                CompactError::InvalidInput("truncated varint")
            ));
        }
    }

    #[test]
    fn decode_u64_overlong_input() {
        let cases: &[&[u8]] = &[
            &[0x80, 0x00],
            &[0x81, 0x00],
            &[0x80, 0x80, 0x00],
            &[0x80, 0x80, 0x80, 0x80, 0x00],
        ];

        for &case in cases {
            let err = decode_u64(case).unwrap_err();
            assert!(matches!(err, CompactError::InvalidInput("overlong varint")));
        }
    }

    #[test]
    fn decode_u64_overflow() {
        let cases: &[&[u8]] = &[
            &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x02],
            &[
                0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00,
            ],
        ];

        for &case in cases {
            let err = decode_u64(case).unwrap_err();
            assert!(matches!(err, CompactError::InvalidInput("varint overflow")));
        }
    }

    #[test]
    fn decode_u64_exact_values() {
        let encoded = [
            0x00, 0x01, 0x7f, 0x80, 0x01, 0xac, 0x02, 0xff, 0xff, 0xff, 0xff, 0x0f,
        ];
        let decoded = decode_u64(&encoded).unwrap();

        assert_eq!(decoded, vec![0, 1, 127, 128, 300, u32::MAX as u64]);
    }

    #[test]
    fn encode_decode_roundtrip() {
        let values = [
            0,
            1,
            2,
            3,
            10,
            63,
            64,
            65,
            126,
            127,
            128,
            129,
            255,
            300,
            16_383,
            16_384,
            1 << 20,
            u32::MAX as u64,
            (u32::MAX as u64) + 1,
            u64::MAX,
        ];

        let encoded = encode_u64(&values);
        let decoded = decode_u64(&encoded).unwrap();

        assert_eq!(decoded, values);
    }
}
