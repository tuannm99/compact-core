/// Metadata for one independently decodable streaming block.
///
/// v0.2 writes blocks sequentially first. A future footer index can store a
/// list of these records for random access, but the fields are useful
/// immediately for `inspect`, corruption reporting, and throughput benchmarks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockMetadata {
    /// Zero-based block number in physical file order.
    pub block_index: u64,
    /// Byte offset where the encoded block starts.
    pub encoded_offset: u64,
    /// Number of JSONL rows represented by this block.
    pub row_count: u64,
    /// Raw JSONL bytes accumulated before columnar compression.
    pub uncompressed_size: u64,
    /// Encoded bytes written for this block, including the block envelope.
    pub compressed_size: u64,
    /// CRC32 of the encoded block payload.
    pub checksum: u32,
}

impl BlockMetadata {
    /// Return `true` when this block carries no rows.
    ///
    /// Empty blocks should normally not be written, but this helper gives
    /// inspect and validation code one consistent definition if a malformed
    /// file contains such a block.
    pub fn is_empty(self) -> bool {
        self.row_count == 0
    }
}

#[cfg(test)]
mod tests {
    use super::BlockMetadata;

    #[test]
    fn empty_metadata_is_identified_by_row_count() {
        let metadata = BlockMetadata {
            block_index: 0,
            encoded_offset: 0,
            row_count: 0,
            uncompressed_size: 0,
            compressed_size: 0,
            checksum: 0,
        };

        assert!(metadata.is_empty());
    }

    #[test]
    fn non_empty_metadata_requires_rows() {
        let metadata = BlockMetadata {
            block_index: 7,
            encoded_offset: 1024,
            row_count: 42,
            uncompressed_size: 4096,
            compressed_size: 1024,
            checksum: 0x12_34_56_78,
        };

        assert!(!metadata.is_empty());
    }
}
