//! Append-oriented JSONL stream support for v0.6.
//!
//! Append streams reuse the v0.2 header and block frames but intentionally do
//! not write the `IDX1` footer. A footer is a closed-file index; append logs are
//! recovered by scanning valid frames up to the first partial or corrupt record.

use std::io::{BufRead, Cursor, Write};

use crate::primitives::crc32;
use crate::schema::Schema;
use crate::streaming::{BLOCK_MAGIC_V1, BlockMetadata, BlockOptions, JsonlBlockWriter};
use crate::{Codec, CompactError, MAGIC_V1, MAGIC_V2, Result, VERSION_V2, framing};

const STREAM_HEADER_LEN: usize = 4 + 1 + 1 + 4;
const FRAME_HEADER_LEN: usize = 4 + 1 + 1 + 8 + 4;
const FRAME_PAYLOAD_LEN_OFFSET: usize = 4 + 1 + 1;
const STREAM_FLAGS: u8 = 0;
const STREAM_HEADER_EXTENSION_LEN: u32 = 0;

/// Recovery result for an append stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppendRecovery {
    pub valid_len: u64,
    pub blocks: Vec<BlockMetadata>,
    pub total_rows: u64,
    pub total_uncompressed_size: u64,
    pub total_compressed_size: u64,
    pub truncated_or_corrupt_tail: bool,
}

/// Append writer that leaves the stream open-ended instead of writing `IDX1`.
pub struct JsonlAppendWriter<W: Write> {
    inner: JsonlBlockWriter<W>,
}

impl<W: Write> JsonlAppendWriter<W> {
    /// Start a new append stream and write the v0.2 stream header.
    pub fn new(writer: W, schema: Schema, options: BlockOptions) -> Result<Self> {
        Ok(Self {
            inner: JsonlBlockWriter::new(writer, schema, options)?,
        })
    }

    /// Resume writing after an already recovered valid prefix.
    ///
    /// The caller is responsible for positioning `writer` at
    /// `recovery.valid_len`, or for passing a buffer that already contains the
    /// valid prefix.
    pub fn resume(
        writer: W,
        schema: Schema,
        options: BlockOptions,
        recovery: &AppendRecovery,
    ) -> Result<Self> {
        Ok(Self {
            inner: JsonlBlockWriter::resume_without_header(
                writer,
                schema,
                options,
                recovery.blocks.len() as u64,
                recovery.total_rows,
                recovery.valid_len,
            )?,
        })
    }

    pub fn write_jsonl_line(&mut self, line: &str) -> Result<()> {
        self.inner.write_jsonl_line(line)
    }

    pub fn flush_block(&mut self) -> Result<()> {
        self.inner.flush_block()
    }

    pub fn metadata(&self) -> &[BlockMetadata] {
        self.inner.metadata()
    }

    /// Flush pending rows and return the wrapped writer without closing index.
    pub fn finish(self) -> Result<W> {
        self.inner.finish_without_footer()
    }
}

/// Append JSONL lines to an existing append stream byte buffer.
///
/// If `existing` has a partial or corrupt tail, only the recovered valid prefix
/// is kept before new rows are appended.
pub fn append_jsonl_stream<R: BufRead>(
    existing: &[u8],
    mut input: R,
    schema: Schema,
    options: BlockOptions,
) -> Result<Vec<u8>> {
    let recovery = recover_append_stream(existing)?;
    let mut output = if existing.is_empty() {
        Vec::new()
    } else {
        existing[..recovery.valid_len as usize].to_vec()
    };

    let mut writer = if output.is_empty() {
        JsonlAppendWriter::new(output, schema, options)?
    } else {
        JsonlAppendWriter::resume(output, schema, options, &recovery)?
    };
    let mut line = String::new();

    loop {
        line.clear();
        let read = input.read_line(&mut line)?;
        if read == 0 {
            break;
        }

        writer.write_jsonl_line(&line)?;
    }

    output = writer.finish()?;

    Ok(output)
}

/// Replay a recovered append stream sequentially into JSONL bytes.
pub fn replay_jsonl_append_stream<W: Write>(
    data: &[u8],
    mut output: W,
    schema: Schema,
) -> Result<W> {
    let recovery = recover_append_stream(data)?;
    let prefix = &data[..recovery.valid_len as usize];
    let mut reader = crate::streaming::JsonlBlockReader::new(Cursor::new(prefix), schema)?;

    while let Some(block) = reader.next_block()? {
        output.write_all(block.jsonl.as_bytes())?;
    }

    Ok(output)
}

/// Recover block metadata and valid byte length from an append stream.
pub fn recover_append_stream(data: &[u8]) -> Result<AppendRecovery> {
    if data.is_empty() {
        return Ok(AppendRecovery {
            valid_len: 0,
            blocks: Vec::new(),
            total_rows: 0,
            total_uncompressed_size: 0,
            total_compressed_size: 0,
            truncated_or_corrupt_tail: false,
        });
    }

    if data.len() < STREAM_HEADER_LEN {
        return Ok(empty_corrupt_recovery());
    }

    if data[..4] != MAGIC_V2
        || data[4] != VERSION_V2
        || data[5] != STREAM_FLAGS
        || u32::from_le_bytes(
            data[6..10]
                .try_into()
                .expect("fixed append stream header length"),
        ) != STREAM_HEADER_EXTENSION_LEN
    {
        return Ok(empty_corrupt_recovery());
    }

    let mut cursor = STREAM_HEADER_LEN;
    let mut blocks = Vec::new();
    let mut expected_block_index = 0u64;
    let mut expected_first_row_index = 0u64;
    let mut truncated_or_corrupt_tail = false;

    while cursor < data.len() {
        let record_start = cursor;
        let Some(header) = data.get(cursor..cursor + FRAME_HEADER_LEN) else {
            truncated_or_corrupt_tail = true;
            break;
        };

        if header[..4] != MAGIC_V1 {
            truncated_or_corrupt_tail = true;
            break;
        }

        let payload_len = u64::from_le_bytes(
            header[FRAME_PAYLOAD_LEN_OFFSET..FRAME_PAYLOAD_LEN_OFFSET + 8]
                .try_into()
                .expect("fixed frame payload length"),
        );
        let Ok(payload_len) = usize::try_from(payload_len) else {
            truncated_or_corrupt_tail = true;
            break;
        };
        let Some(record_len) = FRAME_HEADER_LEN.checked_add(payload_len) else {
            truncated_or_corrupt_tail = true;
            break;
        };
        let Some(frame) = data.get(record_start..record_start + record_len) else {
            truncated_or_corrupt_tail = true;
            break;
        };
        let Ok(decoded) = framing::decode_v1(frame) else {
            truncated_or_corrupt_tail = true;
            break;
        };
        if decoded.codec != Codec::ColumnBlock {
            truncated_or_corrupt_tail = true;
            break;
        }

        let Ok(parsed) = parse_append_block_payload(&decoded.payload) else {
            truncated_or_corrupt_tail = true;
            break;
        };
        if parsed.block_index != expected_block_index
            || parsed.first_row_index != expected_first_row_index
        {
            truncated_or_corrupt_tail = true;
            break;
        }

        let metadata = BlockMetadata {
            block_index: parsed.block_index,
            encoded_offset: record_start as u64,
            row_count: parsed.row_count,
            uncompressed_size: parsed.raw_size,
            compressed_size: record_len as u64,
            checksum: crc32::checksum(&decoded.payload),
        };
        expected_block_index += 1;
        expected_first_row_index += metadata.row_count;
        cursor += record_len;
        blocks.push(metadata);
    }

    let total_rows = blocks.iter().map(|block| block.row_count).sum();
    let total_uncompressed_size = blocks.iter().map(|block| block.uncompressed_size).sum();
    let total_compressed_size = blocks.iter().map(|block| block.compressed_size).sum();

    Ok(AppendRecovery {
        valid_len: cursor as u64,
        blocks,
        total_rows,
        total_uncompressed_size,
        total_compressed_size,
        truncated_or_corrupt_tail,
    })
}

struct AppendBlockPayload {
    block_index: u64,
    first_row_index: u64,
    row_count: u64,
    raw_size: u64,
}

fn parse_append_block_payload(payload: &[u8]) -> Result<AppendBlockPayload> {
    let mut cursor = 0usize;
    let magic = read_slice(payload, &mut cursor, BLOCK_MAGIC_V1.len())?;
    if magic != BLOCK_MAGIC_V1 {
        return Err(CompactError::InvalidInput("invalid block magic"));
    }

    let block_index = read_u64(payload, &mut cursor)?;
    let first_row_index = read_u64(payload, &mut cursor)?;
    let row_count = read_u64(payload, &mut cursor)?;
    let raw_size = read_u64(payload, &mut cursor)?;
    let column_block_len = read_u64(payload, &mut cursor)?;
    let column_block_len = usize::try_from(column_block_len)
        .map_err(|_| CompactError::InvalidInput("block column payload length is too large"))?;
    let _column_block = read_slice(payload, &mut cursor, column_block_len)?;

    if cursor != payload.len() {
        return Err(CompactError::InvalidInput(
            "block payload has trailing bytes",
        ));
    }

    Ok(AppendBlockPayload {
        block_index,
        first_row_index,
        row_count,
        raw_size,
    })
}

fn empty_corrupt_recovery() -> AppendRecovery {
    AppendRecovery {
        valid_len: 0,
        blocks: Vec::new(),
        total_rows: 0,
        total_uncompressed_size: 0,
        total_compressed_size: 0,
        truncated_or_corrupt_tail: true,
    }
}

fn read_slice<'a>(data: &'a [u8], cursor: &mut usize, len: usize) -> Result<&'a [u8]> {
    let end = cursor
        .checked_add(len)
        .ok_or(CompactError::InvalidInput("append block length overflow"))?;
    data.get(*cursor..end)
        .inspect(|_| *cursor = end)
        .ok_or(CompactError::InvalidInput("append block is truncated"))
}

fn read_u64(data: &[u8], cursor: &mut usize) -> Result<u64> {
    let bytes = read_slice(data, cursor, 8)?;

    Ok(u64::from_le_bytes(
        bytes
            .try_into()
            .expect("read_slice returned exactly eight bytes"),
    ))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{append_jsonl_stream, recover_append_stream, replay_jsonl_append_stream};
    use crate::streaming::BlockOptions;

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
    fn append_stream_replays_sequentially() {
        let first = append_jsonl_stream(
            &[],
            Cursor::new("{\"ts\":100}\n{\"ts\":101}\n"),
            schema(),
            BlockOptions {
                max_rows_per_block: 2,
                max_uncompressed_bytes_per_block: 1024,
            },
        )
        .unwrap();
        let second = append_jsonl_stream(
            &first,
            Cursor::new("{\"ts\":102}\n"),
            schema(),
            BlockOptions {
                max_rows_per_block: 2,
                max_uncompressed_bytes_per_block: 1024,
            },
        )
        .unwrap();
        let decoded = replay_jsonl_append_stream(&second, Vec::new(), schema()).unwrap();

        assert_eq!(
            String::from_utf8(decoded).unwrap(),
            "{\"ts\":100}\n{\"ts\":101}\n{\"ts\":102}\n"
        );
    }

    #[test]
    fn recovery_reports_valid_prefix_before_truncated_tail() {
        let data = append_jsonl_stream(
            &[],
            Cursor::new("{\"ts\":100}\n{\"ts\":101}\n{\"ts\":102}\n"),
            schema(),
            BlockOptions {
                max_rows_per_block: 1,
                max_uncompressed_bytes_per_block: 1024,
            },
        )
        .unwrap();
        let truncated = &data[..data.len() - 3];
        let recovery = recover_append_stream(truncated).unwrap();

        assert_eq!(recovery.blocks.len(), 2);
        assert_eq!(recovery.total_rows, 2);
        assert!(recovery.truncated_or_corrupt_tail);
    }

    #[test]
    fn append_after_corrupt_tail_keeps_only_valid_prefix() {
        let mut data = append_jsonl_stream(
            &[],
            Cursor::new("{\"ts\":100}\n{\"ts\":101}\n"),
            schema(),
            BlockOptions {
                max_rows_per_block: 1,
                max_uncompressed_bytes_per_block: 1024,
            },
        )
        .unwrap();
        *data.last_mut().unwrap() ^= 0xff;

        let appended = append_jsonl_stream(
            &data,
            Cursor::new("{\"ts\":102}\n"),
            schema(),
            BlockOptions {
                max_rows_per_block: 1,
                max_uncompressed_bytes_per_block: 1024,
            },
        )
        .unwrap();
        let decoded = replay_jsonl_append_stream(&appended, Vec::new(), schema()).unwrap();

        assert_eq!(
            String::from_utf8(decoded).unwrap(),
            "{\"ts\":100}\n{\"ts\":102}\n"
        );
    }
}
