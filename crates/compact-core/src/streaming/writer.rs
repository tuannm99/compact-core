//! Streaming JSONL block writer.
//!
//! This module will own the bounded-memory encode path. The next implementation
//! step is to extract the current one-shot JSONL column-block encoder so a
//! writer can encode one row group at a time.

use std::io::Write;

use crate::io::encode_jsonl_row_group;
use crate::primitives::crc32;
use crate::schema::Schema;
use crate::streaming::{BLOCK_MAGIC_V1, BlockMetadata, BlockOptions};
use crate::{Codec, CompactError, MAGIC_V2, Result, VERSION_V2, framing};

const STREAM_FLAGS: u8 = 0;
const STREAM_HEADER_LEN: u32 = 0;
const FILE_HEADER_LEN: u64 = 4 + 1 + 1 + 4;

/// Streaming JSONL writer for the v0.2 block format.
///
/// The writer owns buffering so callers do not accidentally build a full-file
/// string before encoding. Rows are accumulated only until `BlockOptions`
/// limits are reached, then the current row group is encoded and written as one
/// independently decodable block.
pub struct JsonlBlockWriter<W: Write> {
    writer: W,
    schema: Schema,
    options: BlockOptions,
    current: RowGroupBuffer,
    blocks_written: u64,
    rows_written: u64,
    bytes_written: u64,
    metadata: Vec<BlockMetadata>,
}

impl<W: Write> JsonlBlockWriter<W> {
    /// Create a writer and immediately emit the v0.2 file header.
    ///
    /// Writing the header in `new` makes a successfully constructed writer a
    /// valid empty stream after `finish`. If header write fails, callers never
    /// receive a partially initialized writer.
    pub fn new(mut writer: W, schema: Schema, options: BlockOptions) -> Result<Self> {
        let options = options.validate()?;

        writer.write_all(&MAGIC_V2)?;
        writer.write_all(&[VERSION_V2])?;
        writer.write_all(&[STREAM_FLAGS])?;
        writer.write_all(&STREAM_HEADER_LEN.to_le_bytes())?;

        Ok(Self {
            writer,
            schema,
            options,
            current: RowGroupBuffer::default(),
            blocks_written: 0,
            rows_written: 0,
            bytes_written: FILE_HEADER_LEN,
            metadata: Vec::new(),
        })
    }

    /// Buffer one JSONL row and flush automatically if block limits are reached.
    ///
    /// `line` may include one trailing newline. The writer normalizes accepted
    /// rows to exactly one trailing `\n` because the existing JSONL decoder
    /// renders one object per line with a final newline. Blank lines are ignored
    /// to match the current one-shot encoder.
    pub fn write_jsonl_line(&mut self, line: &str) -> Result<()> {
        if line.trim().is_empty() {
            return Ok(());
        }

        let line = normalize_jsonl_line(line)?;
        if line.len() > self.options.max_uncompressed_bytes_per_block {
            return Err(CompactError::InvalidInput(
                "jsonl row exceeds max uncompressed bytes per block",
            ));
        }

        if self.current.would_exceed(&line, self.options) {
            self.flush_block()?;
        }

        self.current.push_line(&line);

        if self.current.reached_limit(self.options) {
            self.flush_block()?;
        }

        Ok(())
    }

    /// Encode and write the current row group as one framed block.
    ///
    /// Empty flushes are no-ops. This is important because callers may call
    /// `finish` after a row-limit flush already wrote the final block.
    pub fn flush_block(&mut self) -> Result<()> {
        if self.current.is_empty() {
            return Ok(());
        }

        let row_group = encode_jsonl_row_group(self.current.as_str(), &self.schema)?;
        let block_payload = encode_block_payload(
            self.blocks_written,
            self.rows_written,
            row_group.row_count,
            row_group.raw_bytes,
            &row_group.payload,
        )?;
        let encoded_frame = framing::encode_v1(Codec::ColumnBlock, &block_payload);
        let compressed_size = u64::try_from(encoded_frame.len())
            .map_err(|_| CompactError::InvalidInput("block frame is too large"))?;
        let metadata = BlockMetadata {
            block_index: self.blocks_written,
            encoded_offset: self.bytes_written,
            row_count: usize_to_u64(row_group.row_count, "row count is too large")?,
            uncompressed_size: usize_to_u64(row_group.raw_bytes, "row group is too large")?,
            compressed_size,
            checksum: crc32::checksum(&block_payload),
        };

        self.writer.write_all(&encoded_frame)?;
        self.blocks_written += 1;
        self.rows_written += metadata.row_count;
        self.bytes_written += compressed_size;
        self.metadata.push(metadata);
        self.current.clear();

        Ok(())
    }

    /// Flush remaining rows and return the wrapped writer.
    ///
    /// If this fails, the caller must treat the output as incomplete. The API
    /// returns the writer only after all pending data has been written
    /// successfully.
    pub fn finish(mut self) -> Result<W> {
        self.flush_block()?;

        Ok(self.writer)
    }

    /// Metadata for blocks written so far.
    ///
    /// This is an in-memory inline index useful for tests and future `inspect`.
    /// The first v0.2 writer does not persist a footer index yet because that
    /// would require additional seek/footer design.
    pub fn metadata(&self) -> &[BlockMetadata] {
        &self.metadata
    }
}

#[derive(Default)]
struct RowGroupBuffer {
    data: String,
    row_count: usize,
    raw_bytes: usize,
}

impl RowGroupBuffer {
    fn push_line(&mut self, line: &str) {
        self.data.push_str(line);
        self.row_count += 1;
        self.raw_bytes += line.len();
    }

    fn would_exceed(&self, line: &str, options: BlockOptions) -> bool {
        if self.is_empty() {
            return false;
        }

        self.row_count + 1 > options.max_rows_per_block
            || self.raw_bytes + line.len() > options.max_uncompressed_bytes_per_block
    }

    fn reached_limit(&self, options: BlockOptions) -> bool {
        self.row_count >= options.max_rows_per_block
            || self.raw_bytes >= options.max_uncompressed_bytes_per_block
    }

    fn is_empty(&self) -> bool {
        self.row_count == 0
    }

    fn as_str(&self) -> &str {
        &self.data
    }

    fn clear(&mut self) {
        self.data.clear();
        self.row_count = 0;
        self.raw_bytes = 0;
    }
}

fn normalize_jsonl_line(line: &str) -> Result<String> {
    let line = line.strip_suffix('\n').unwrap_or(line);
    let line = line.strip_suffix('\r').unwrap_or(line);

    if line.contains(['\n', '\r']) {
        return Err(CompactError::InvalidInput(
            "jsonl line must not contain embedded newline",
        ));
    }

    let mut normalized = String::with_capacity(line.len() + 1);
    normalized.push_str(line);
    normalized.push('\n');

    Ok(normalized)
}

fn encode_block_payload(
    block_index: u64,
    first_row_index: u64,
    row_count: usize,
    raw_size: usize,
    column_block: &[u8],
) -> Result<Vec<u8>> {
    let row_count = usize_to_u64(row_count, "row count is too large")?;
    let raw_size = usize_to_u64(raw_size, "row group is too large")?;
    let column_block_len = usize_to_u64(column_block.len(), "column block is too large")?;
    let mut payload = Vec::with_capacity(BLOCK_MAGIC_V1.len() + 8 * 5 + column_block.len());

    payload.extend_from_slice(&BLOCK_MAGIC_V1);
    payload.extend_from_slice(&block_index.to_le_bytes());
    payload.extend_from_slice(&first_row_index.to_le_bytes());
    payload.extend_from_slice(&row_count.to_le_bytes());
    payload.extend_from_slice(&raw_size.to_le_bytes());
    payload.extend_from_slice(&column_block_len.to_le_bytes());
    payload.extend_from_slice(column_block);

    Ok(payload)
}

fn usize_to_u64(value: usize, err: &'static str) -> Result<u64> {
    u64::try_from(value).map_err(|_| CompactError::InvalidInput(err))
}

#[cfg(test)]
mod tests {
    use super::{JsonlBlockWriter, normalize_jsonl_line};
    use crate::streaming::BlockOptions;
    use crate::{CompactError, MAGIC_V2, VERSION_V2};

    fn schema() -> crate::schema::Schema {
        crate::schema::Schema::from_yaml(
            r#"
columns:
  - name: ts
    type: u64
    codec: delta_varint_u64
"#,
        )
        .unwrap()
    }

    #[test]
    fn finish_empty_input_writes_valid_header_only() {
        let writer = JsonlBlockWriter::new(Vec::new(), schema(), BlockOptions::default()).unwrap();
        let output = writer.finish().unwrap();

        assert_eq!(&output[0..4], &MAGIC_V2);
        assert_eq!(output[4], VERSION_V2);
        assert_eq!(output.len(), 10);
    }

    #[test]
    fn one_row_writes_one_block() {
        let mut writer =
            JsonlBlockWriter::new(Vec::new(), schema(), BlockOptions::default()).unwrap();

        writer.write_jsonl_line("{\"ts\":100}").unwrap();
        assert_eq!(writer.metadata().len(), 0);
        let output = writer.finish().unwrap();

        assert!(output.len() > 10);
    }

    #[test]
    fn row_limit_flushes_multiple_blocks() {
        let mut writer = JsonlBlockWriter::new(
            Vec::new(),
            schema(),
            BlockOptions {
                max_rows_per_block: 2,
                max_uncompressed_bytes_per_block: 1024,
            },
        )
        .unwrap();

        writer.write_jsonl_line("{\"ts\":100}").unwrap();
        writer.write_jsonl_line("{\"ts\":101}").unwrap();
        writer.write_jsonl_line("{\"ts\":102}").unwrap();

        assert_eq!(writer.metadata().len(), 1);
        assert_eq!(writer.metadata()[0].row_count, 2);
        let output = writer.finish().unwrap();

        assert!(output.len() > 10);
    }

    #[test]
    fn byte_limit_flushes_before_next_row() {
        let mut writer = JsonlBlockWriter::new(
            Vec::new(),
            schema(),
            BlockOptions {
                max_rows_per_block: 100,
                max_uncompressed_bytes_per_block: 21,
            },
        )
        .unwrap();

        writer.write_jsonl_line("{\"ts\":100}").unwrap();
        writer.write_jsonl_line("{\"ts\":101}").unwrap();

        assert_eq!(writer.metadata().len(), 1);
        assert_eq!(writer.metadata()[0].uncompressed_size, 11);
    }

    #[test]
    fn oversized_single_row_is_rejected() {
        let mut writer = JsonlBlockWriter::new(
            Vec::new(),
            schema(),
            BlockOptions {
                max_rows_per_block: 100,
                max_uncompressed_bytes_per_block: 4,
            },
        )
        .unwrap();
        let err = writer.write_jsonl_line("{\"ts\":100}").unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("jsonl row exceeds max uncompressed bytes per block")
        ));
    }

    #[test]
    fn embedded_newline_is_rejected() {
        let err = normalize_jsonl_line("{\"ts\":100}\n{\"ts\":101}").unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("jsonl line must not contain embedded newline")
        ));
    }
}
