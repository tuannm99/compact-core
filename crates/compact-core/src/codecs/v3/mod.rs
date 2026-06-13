//! Codecs introduced for CMP3 columns.

use crate::format::v3::ColumnChunkMetadata;

pub mod boolean;
pub mod numeric;
pub mod string;

/// One encoded CMP3 column before row-group framing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedColumnChunk {
    pub metadata: ColumnChunkMetadata,
    pub payload: Vec<u8>,
}
