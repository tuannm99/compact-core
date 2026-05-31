use crate::CompactError;

/// Encode unsigned 64-bit integers with base-128 varint encoding.
///
/// A varint stores seven payload bits per byte. The high bit (`0x80`) is a
/// continuation flag: it is set while more bytes are needed and cleared on the
/// final byte for the current integer.
pub fn encode_u64(values: &[u64]) -> Vec<u8> {
    // `u64::MAX` needs at most 10 base-128 bytes, so this avoids repeated
    // growth for the worst case while still allowing small values to shrink.
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

/// Decode canonical base-128 varints into unsigned 64-bit integers.
///
/// The decoder is intentionally strict. It rejects truncated streams,
/// encodings that need more than 64 payload bits, and overlong encodings such
/// as `[0x80, 0x00]` for zero. Keeping only one valid byte sequence per integer
/// matters for checksums, file comparison, and corruption detection.
pub fn decode_u64(data: &[u8]) -> Result<Vec<u64>, CompactError> {
    let mut out = Vec::new();

    let mut value = 0u64;
    let mut shift = 0u32;
    let mut bytes_read = 0usize;

    for &byte in data {
        let part = (byte & 0x7f) as u64;

        // A `u64` can use 9 full groups of 7 bits plus 1 final payload bit.
        // At shift 63, only payload values 0 or 1 fit, and byte 10 must be the
        // terminal byte. Anything larger would require more than 64 bits.
        if shift == 63 && (part > 1 || byte & 0x80 != 0) {
            return Err(CompactError::InvalidInput("varint overflow"));
        }

        if shift > 63 {
            return Err(CompactError::InvalidInput("varint overflow"));
        }

        value |= part << shift;
        bytes_read += 1;

        if byte & 0x80 == 0 {
            if bytes_read != encoded_len(value) {
                return Err(CompactError::InvalidInput("overlong varint"));
            }

            out.push(value);
            value = 0;
            shift = 0;
            bytes_read = 0;
        } else {
            shift += 7;
        }
    }

    if shift != 0 {
        return Err(CompactError::InvalidInput("truncated varint"));
    }

    Ok(out)
}

fn encoded_len(mut value: u64) -> usize {
    let mut len = 1;

    while value >= 0x80 {
        value >>= 7;
        len += 1;
    }

    len
}

#[cfg(test)]
mod tests {
    use super::{decode_u64, encode_u64};
    use crate::CompactError;

    #[test]
    fn encode_u64_edgecase() {
        assert_eq!(encode_u64(&[]), Vec::<u8>::new());
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
