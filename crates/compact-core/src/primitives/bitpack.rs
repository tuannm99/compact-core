use crate::CompactError;

/// Pack `u64` values using a fixed number of bits per value.
///
/// This primitive does not write a header. The caller must store `bit_width`
/// and the number of values in the surrounding frame or codec metadata, then
/// pass the same values back to `decode_u64`.
pub fn encode_u64(values: &[u64], bit_width: u8) -> Result<Vec<u8>, CompactError> {
    validate_bit_width(bit_width)?;

    if values.is_empty() || bit_width == 0 {
        if values.iter().any(|&value| value != 0) {
            return Err(CompactError::InvalidInput(
                "non-zero value cannot fit in zero bits",
            ));
        }

        return Ok(Vec::new());
    }

    let max_value = max_value_for_width(bit_width);

    if values.iter().any(|&value| value > max_value) {
        return Err(CompactError::InvalidInput("value exceeds bit width"));
    }

    let total_bits =
        values
            .len()
            .checked_mul(bit_width as usize)
            .ok_or(CompactError::InvalidInput(
                "bit-packed output length overflow",
            ))?;
    let mut out = Vec::with_capacity(total_bits.div_ceil(8));

    let mut current_byte = 0u8;
    let mut bits_in_current_byte = 0u8;

    for &value in values {
        // Values are written least-significant bit first. This matches the
        // little-endian style used by the varint primitive and keeps extraction
        // cheap when reading back from a byte stream.
        for bit_index in 0..bit_width {
            let bit = ((value >> bit_index) & 1) as u8;
            current_byte |= bit << bits_in_current_byte;
            bits_in_current_byte += 1;

            if bits_in_current_byte == 8 {
                out.push(current_byte);
                current_byte = 0;
                bits_in_current_byte = 0;
            }
        }
    }

    if bits_in_current_byte > 0 {
        out.push(current_byte);
    }

    Ok(out)
}

/// Unpack fixed-width `u64` values from bytes produced by `encode_u64`.
///
/// `value_count` is required because the last output byte may contain padding
/// bits. Padding bits must be zero; otherwise the stream is treated as
/// malformed instead of silently accepting corrupted data.
pub fn decode_u64(
    data: &[u8],
    bit_width: u8,
    value_count: usize,
) -> Result<Vec<u64>, CompactError> {
    validate_bit_width(bit_width)?;

    let total_bits =
        value_count
            .checked_mul(bit_width as usize)
            .ok_or(CompactError::InvalidInput(
                "bit-packed input length overflow",
            ))?;
    let expected_len = total_bits.div_ceil(8);

    if data.len() != expected_len {
        return Err(CompactError::InvalidInput(
            "bit-packed input length does not match value count",
        ));
    }

    if bit_width == 0 {
        return Ok(vec![0; value_count]);
    }

    reject_non_zero_padding(data, total_bits)?;

    let mut values = Vec::with_capacity(value_count);
    let mut bit_offset = 0usize;

    for _ in 0..value_count {
        let mut value = 0u64;

        for bit_index in 0..bit_width {
            let source_byte = data[bit_offset / 8];
            let source_bit = (source_byte >> (bit_offset % 8)) & 1;
            value |= (source_bit as u64) << bit_index;
            bit_offset += 1;
        }

        values.push(value);
    }

    Ok(values)
}

fn validate_bit_width(bit_width: u8) -> Result<(), CompactError> {
    if bit_width > 64 {
        return Err(CompactError::InvalidInput("bit width must be <= 64"));
    }

    Ok(())
}

fn max_value_for_width(bit_width: u8) -> u64 {
    if bit_width == 64 {
        u64::MAX
    } else if bit_width == 0 {
        0
    } else {
        (1u64 << bit_width) - 1
    }
}

fn reject_non_zero_padding(data: &[u8], used_bits: usize) -> Result<(), CompactError> {
    let padding_bits = data.len() * 8 - used_bits;

    if padding_bits == 0 {
        return Ok(());
    }

    let used_bits_in_last_byte = used_bits % 8;
    let padding_mask = if used_bits_in_last_byte == 0 {
        0
    } else {
        u8::MAX << used_bits_in_last_byte
    };

    if data.last().is_some_and(|last| last & padding_mask != 0) {
        return Err(CompactError::InvalidInput(
            "bit-packed padding bits must be zero",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{decode_u64, encode_u64};
    use crate::CompactError;

    #[test]
    fn bitpack_zero_values() {
        let encoded = encode_u64(&[0, 0, 0], 0).unwrap();
        let decoded = decode_u64(&encoded, 0, 3).unwrap();

        assert_eq!(encoded, Vec::<u8>::new());
        assert_eq!(decoded, vec![0, 0, 0]);
    }

    #[test]
    fn bitpack_single_bit_values() {
        let values = [1, 0, 1, 1, 0, 0, 1, 0];
        let encoded = encode_u64(&values, 1).unwrap();
        let decoded = decode_u64(&encoded, 1, values.len()).unwrap();

        assert_eq!(encoded, vec![0b0100_1101]);
        assert_eq!(decoded, values);
    }

    #[test]
    fn bitpack_crosses_byte_boundary() {
        let values = [0, 1, 2, 3, 4, 5, 6, 7];
        let encoded = encode_u64(&values, 3).unwrap();
        let decoded = decode_u64(&encoded, 3, values.len()).unwrap();

        assert_eq!(encoded.len(), 3);
        assert_eq!(decoded, values);
    }

    #[test]
    fn bitpack_roundtrip_full_width() {
        let values = [0, 1, u64::MAX, 0x0123_4567_89ab_cdef];
        let encoded = encode_u64(&values, 64).unwrap();
        let decoded = decode_u64(&encoded, 64, values.len()).unwrap();

        assert_eq!(encoded.len(), values.len() * 8);
        assert_eq!(decoded, values);
    }

    #[test]
    fn bitpack_rejects_value_that_does_not_fit() {
        let err = encode_u64(&[8], 3).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("value exceeds bit width")
        ));
    }

    #[test]
    fn bitpack_rejects_invalid_width() {
        let err = encode_u64(&[0], 65).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("bit width must be <= 64")
        ));
    }

    #[test]
    fn bitpack_rejects_wrong_input_length() {
        let err = decode_u64(&[0xff, 0x00], 3, 1).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("bit-packed input length does not match value count")
        ));
    }

    #[test]
    fn bitpack_rejects_non_zero_padding() {
        let err = decode_u64(&[0b1111_1000], 3, 1).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("bit-packed padding bits must be zero")
        ));
    }
}
