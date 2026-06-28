//! Resource limits applied while decoding untrusted input.

/// Maximum number of bytes a primitive decoder may materialize.
pub(crate) const MAX_DECODED_BYTES: usize = 256 * 1024 * 1024;

/// Maximum number of logical values accepted from file metadata.
pub(crate) const MAX_DECODED_VALUES: usize = 16 * 1024 * 1024;

/// Maximum number of entries accepted for format indexes and dictionaries.
pub(crate) const MAX_COLLECTION_ENTRIES: usize = 1_000_000;

/// Maximum compressed payload size accepted for one streaming frame.
pub(crate) const MAX_ENCODED_BLOCK_BYTES: usize = 256 * 1024 * 1024;

/// Maximum number of blocks retained in a streaming footer.
pub(crate) const MAX_STREAM_BLOCKS: usize = 1_000_000;

/// Maximum worker threads accepted by parallel APIs.
pub(crate) const MAX_PARALLEL_WORKERS: usize = 256;
