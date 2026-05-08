use std::io;

use thiserror::Error;

pub const MAGIC_V1: [u8; 4] = *b"CMP1";
pub const VERSION_V1: u8 = 1;

pub type Result<T> = std::result::Result<T, CompactError>;

#[derive(Debug, Error)]
pub enum CompactError {
    #[error("invalid input: {0}")]
    InvalidInput(&'static str),
    #[error("unsupported feature: {0}")]
    Unsupported(&'static str),
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
}

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn checksum32(input: &[u8]) -> u32 {
    crc32fast::hash(input)
}

#[cfg(test)]
mod tests {
    use super::{MAGIC_V1, VERSION_V1, checksum32, crate_version};

    #[test]
    fn version_is_set() {
        assert_eq!(crate_version(), "0.1.0");
    }

    #[test]
    fn frame_constants_match_v1() {
        assert_eq!(MAGIC_V1, *b"CMP1");
        assert_eq!(VERSION_V1, 1);
    }

    #[test]
    fn checksum_is_stable() {
        assert_eq!(checksum32(b"compact"), 0x84_a0_0b_bf);
    }
}
