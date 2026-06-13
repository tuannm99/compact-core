//! Dense one-bit-per-value storage used by CMP3 boolean and validity columns.
//!
//! Bits are written least-significant-bit first within each byte. The final
//! byte is padded with zero bits. Decoders reject non-zero padding so malformed
//! input cannot encode hidden values beyond the declared logical length.

use crate::{CompactError, Result};

/// Encode logical bits into a compact byte sequence.
pub fn encode(bits: &[bool]) -> Vec<u8> {
    let mut out = vec![0u8; bits.len().div_ceil(8)];

    for (index, &bit) in bits.iter().enumerate() {
        if bit {
            out[index / 8] |= 1 << (index % 8);
        }
    }

    out
}

/// Decode exactly `bit_count` logical bits.
///
/// The byte length must be exactly `ceil(bit_count / 8)`. Accepting longer
/// input would make payload boundaries ambiguous; accepting shorter input
/// would silently manufacture missing values.
pub fn decode(data: &[u8], bit_count: usize) -> Result<Vec<bool>> {
    let expected_len = bit_count.div_ceil(8);
    if data.len() != expected_len {
        return Err(CompactError::InvalidInput(
            "bitmap length does not match bit count",
        ));
    }

    reject_non_zero_padding(data, bit_count)?;

    Ok((0..bit_count)
        .map(|index| data[index / 8] & (1 << (index % 8)) != 0)
        .collect())
}

fn reject_non_zero_padding(data: &[u8], bit_count: usize) -> Result<()> {
    let used_bits_in_last_byte = bit_count % 8;
    if used_bits_in_last_byte == 0 {
        return Ok(());
    }

    let padding_mask = u8::MAX << used_bits_in_last_byte;
    if data.last().is_some_and(|last| last & padding_mask != 0) {
        return Err(CompactError::InvalidInput(
            "bitmap padding bits must be zero",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{decode, encode};
    use crate::CompactError;

    #[test]
    fn empty_bitmap_roundtrips() {
        let encoded = encode(&[]);
        let decoded = decode(&encoded, 0).unwrap();

        assert!(encoded.is_empty());
        assert!(decoded.is_empty());
    }

    #[test]
    fn bitmap_roundtrips_across_byte_boundaries() {
        let bits = [true, false, true, true, false, false, true, false, true];
        let encoded = encode(&bits);
        let decoded = decode(&encoded, bits.len()).unwrap();

        assert_eq!(encoded, vec![0b0100_1101, 0b0000_0001]);
        assert_eq!(decoded, bits);
    }

    #[test]
    fn bitmap_rejects_short_and_long_payloads() {
        for data in [&[][..], &[0, 0][..]] {
            let err = decode(data, 1).unwrap_err();

            assert!(matches!(
                err,
                CompactError::InvalidInput("bitmap length does not match bit count")
            ));
        }
    }

    #[test]
    fn bitmap_rejects_non_zero_padding() {
        let err = decode(&[0b1000_0001], 1).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("bitmap padding bits must be zero")
        ));
    }
}
