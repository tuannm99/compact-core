use std::io as std_io;

use thiserror::Error;

mod codecs;
mod framing;
mod io;
mod primitives;
mod schema;
mod transforms;

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
    Io(#[from] std_io::Error),
}

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn checksum32(input: &[u8]) -> u32 {
    crc32fast::hash(input)
}

pub enum ValueType {
    RawBytes,
    U32,
    U64,
    I32,
    I64,
}

pub enum Transform {
    None,
    Delta,
    /*REVIEWER [BLOCKER][CORRECTNESS]: delta transform is exposed before it has a safe implementation path.
    WHY: the crate publishes `Transform::Delta`, but the current delta codec functions silently discard input and return empty output. Wiring this variant into any encode/decode flow would corrupt data instead of failing loudly.
    FIX: hide this variant until delta is implemented, or ensure any public entrypoint using it returns `CompactError::Unsupported("delta transform")`.
    */
    ZigZag,
}

pub enum Codec {
    Rle,
    Huffman,
    Lz77,
}

pub struct EncodeConfig {
    pub value_type: ValueType,
    pub transform: Transform,
    /*REVIEWER [MAJOR][API_DESIGN]: this public config advertises capabilities the crate cannot execute yet.
    WHY: callers can select unsupported codecs/transforms such as `Huffman`, `Lz77`, or the current stubbed `Delta`, but there is no public API that validates the choice or reports an explicit unsupported error.
    FIX: keep this type private until the execution pipeline exists, or add a public encode/decode API that validates each combination and returns `CompactError::Unsupported` for unimplemented cases.
    */
    pub codec: Codec,
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
