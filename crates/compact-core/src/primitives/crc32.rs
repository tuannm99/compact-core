/// Compute the IEEE CRC32 checksum used by common storage formats.
///
/// CRC32 is an integrity check, not a security primitive. It is useful for
/// detecting accidental corruption in frames or compressed payloads, but it
/// must not be used to authenticate untrusted data.
pub fn checksum(data: &[u8]) -> u32 {
    crc32fast::hash(data)
}

/// Verify that `data` matches an expected CRC32 checksum.
///
/// Keeping this as a small helper makes frame readers easier to read: they can
/// parse bytes, compute the checksum, and return a typed error at the frame
/// layer when this function returns `false`.
pub fn verify(data: &[u8], expected: u32) -> bool {
    checksum(data) == expected
}

#[cfg(test)]
mod tests {
    use super::{checksum, verify};

    #[test]
    fn crc32_matches_standard_test_vector() {
        assert_eq!(checksum(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn crc32_empty_input_is_stable() {
        assert_eq!(checksum(b""), 0);
    }

    #[test]
    fn crc32_verify_accepts_matching_checksum() {
        assert!(verify(b"compact", checksum(b"compact")));
    }

    #[test]
    fn crc32_verify_rejects_mismatch() {
        assert!(!verify(b"compact", 0));
    }
}
