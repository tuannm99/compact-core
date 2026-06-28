//! Sequential streaming block engine for the v0.2 file format.
//!
//! v0.1 encodes a JSONL file as one columnar frame, which means the caller must
//! hold the whole input and output in memory. v0.2 fixes that by splitting the
//! input into independent row groups. Each row group will become one block with
//! its own byte ranges, row count, sizes, and checksum metadata.
//!
//! This module starts with the stable configuration and metadata types. The
//! writer and reader modules will use these types when the row-group encoder is
//! extracted from the current one-shot JSONL path.

use std::io::{BufRead, Read, Write};

use crate::schema::Schema;
use crate::{CompactError, Result};

pub mod append;
pub mod metadata;
pub mod options;
pub mod reader;
pub mod rolling;
pub mod snapshot;
pub mod writer;

pub use append::{
    AppendRecovery, JsonlAppendWriter, append_jsonl_stream, recover_append_stream,
    replay_jsonl_append_stream,
};
pub use metadata::BlockMetadata;
pub use options::BlockOptions;
pub use reader::{DecodedBlock, JsonlBlockReader, StreamInspect, inspect_jsonl_stream};
pub use rolling::{RollingOptions, roll_jsonl_append_segments};
pub use snapshot::{Snapshot, decode_snapshot, encode_snapshot};
pub use writer::JsonlBlockWriter;

/// Magic for a v0.2 block payload inside a framed stream.
pub(crate) const BLOCK_MAGIC_V1: [u8; 4] = *b"BLK1";

/// Magic for the persisted v0.2 footer index.
pub(crate) const INDEX_MAGIC_V1: [u8; 4] = *b"IDX1";

/// Encode JSONL from a buffered reader into a v0.2 block stream.
///
/// This is the high-level API most callers should use. It reads one JSONL line
/// at a time, hands that line to `JsonlBlockWriter`, and relies on
/// `BlockOptions` to decide when a block is flushed. The caller does not need to
/// allocate a full input string.
pub fn encode_jsonl_stream<R: BufRead, W: Write>(
    mut input: R,
    output: W,
    schema: Schema,
    options: BlockOptions,
) -> Result<W> {
    let options = options.validate()?;
    let max_line_len = options.max_uncompressed_bytes_per_block;
    let mut writer = JsonlBlockWriter::new(output, schema, options)?;

    while let Some(line) = read_bounded_jsonl_line(&mut input, max_line_len)? {
        writer.write_jsonl_line(&line)?;
    }

    writer.finish()
}

/// Decode a v0.2 JSONL block stream into a writer.
///
/// Blocks are decoded and written sequentially. The function stops on the first
/// malformed block and returns an error, so callers never receive a successful
/// result for partially trusted output.
pub fn decode_jsonl_stream<R: Read, W: Write>(
    input: R,
    mut output: W,
    schema: Schema,
) -> Result<W> {
    let mut reader = JsonlBlockReader::new(input, schema)?;

    while let Some(block) = reader.next_block()? {
        output.write_all(block.jsonl.as_bytes())?;
    }

    Ok(output)
}

/// Read one UTF-8 line without allowing an unterminated row to grow without
/// bound. One extra byte is accepted so CRLF and a missing final newline can
/// reach the writer's normalization check.
pub(crate) fn read_bounded_jsonl_line<R: BufRead>(
    input: &mut R,
    max_line_len: usize,
) -> Result<Option<String>> {
    let allocation_limit = max_line_len
        .checked_add(1)
        .ok_or(CompactError::InvalidInput("jsonl row limit is too large"))?;
    let mut bytes = Vec::new();

    loop {
        let available = input.fill_buf()?;
        if available.is_empty() {
            if bytes.is_empty() {
                return Ok(None);
            }
            break;
        }

        let consumed = available
            .iter()
            .position(|&byte| byte == b'\n')
            .map_or(available.len(), |index| index + 1);
        if bytes.len().saturating_add(consumed) > allocation_limit {
            return Err(CompactError::InvalidInput(
                "jsonl row exceeds max uncompressed bytes per block",
            ));
        }
        bytes.extend_from_slice(&available[..consumed]);
        input.consume(consumed);

        if bytes.last() == Some(&b'\n') {
            break;
        }
    }

    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| CompactError::InvalidInput("jsonl row must be utf-8"))
}

/// Inspect a v0.2 JSONL block stream without requiring a schema.
///
/// This scans block envelopes and validates frame checksums, but it does not
/// decode column values. That keeps `compact inspect` useful for metadata and
/// corruption checks without forcing users to provide a schema.
pub fn inspect_stream<R: Read>(input: R) -> Result<StreamInspect> {
    inspect_jsonl_stream(input)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{BlockOptions, decode_jsonl_stream, encode_jsonl_stream, inspect_stream};

    fn schema() -> crate::schema::Schema {
        crate::schema::Schema::from_yaml(
            r#"
columns:
  - name: ts
    type: u64
    codec: delta_varint_u64
  - name: level
    type: string
    codec: rle
"#,
        )
        .unwrap()
    }

    #[test]
    fn stream_helpers_roundtrip_jsonl() {
        let input = "{\"ts\":100,\"level\":\"INFO\"}\n{\"ts\":101,\"level\":\"WARN\"}\n";
        let encoded = encode_jsonl_stream(
            Cursor::new(input),
            Vec::new(),
            schema(),
            BlockOptions::default(),
        )
        .unwrap();
        let decoded = decode_jsonl_stream(Cursor::new(encoded), Vec::new(), schema()).unwrap();

        assert_eq!(String::from_utf8(decoded).unwrap(), input);
    }

    #[test]
    fn stream_helpers_roundtrip_multiple_blocks() {
        let input = "{\"ts\":100,\"level\":\"INFO\"}\n{\"ts\":101,\"level\":\"WARN\"}\n{\"ts\":102,\"level\":\"INFO\"}\n";
        let encoded = encode_jsonl_stream(
            Cursor::new(input),
            Vec::new(),
            schema(),
            BlockOptions {
                max_rows_per_block: 2,
                max_uncompressed_bytes_per_block: 1024,
            },
        )
        .unwrap();
        let decoded = decode_jsonl_stream(Cursor::new(encoded), Vec::new(), schema()).unwrap();

        assert_eq!(String::from_utf8(decoded).unwrap(), input);
    }

    #[test]
    fn stream_helpers_accept_empty_input() {
        let encoded = encode_jsonl_stream(
            Cursor::new(""),
            Vec::new(),
            schema(),
            BlockOptions::default(),
        )
        .unwrap();
        let decoded = decode_jsonl_stream(Cursor::new(encoded), Vec::new(), schema()).unwrap();

        assert!(decoded.is_empty());
    }

    #[test]
    fn stream_encoder_rejects_oversized_unterminated_row() {
        let error = encode_jsonl_stream(
            Cursor::new("123456"),
            Vec::new(),
            schema(),
            BlockOptions {
                max_rows_per_block: 1,
                max_uncompressed_bytes_per_block: 4,
            },
        )
        .unwrap_err();

        assert!(matches!(error, crate::CompactError::InvalidInput(_)));
    }

    #[test]
    fn inspect_stream_reports_block_totals() {
        let input = "{\"ts\":100,\"level\":\"INFO\"}\n{\"ts\":101,\"level\":\"WARN\"}\n{\"ts\":102,\"level\":\"INFO\"}\n";
        let encoded = encode_jsonl_stream(
            Cursor::new(input),
            Vec::new(),
            schema(),
            BlockOptions {
                max_rows_per_block: 2,
                max_uncompressed_bytes_per_block: 1024,
            },
        )
        .unwrap();
        let inspect = inspect_stream(Cursor::new(encoded)).unwrap();

        assert_eq!(inspect.blocks.len(), 2);
        assert_eq!(inspect.footer_index.as_ref().map(Vec::len), Some(2));
        assert_eq!(inspect.total_rows, 3);
        assert_eq!(inspect.total_uncompressed_size, input.len() as u64);
        assert!(inspect.total_compressed_size > 0);
    }

    #[test]
    fn decode_stream_stops_on_corrupted_later_block() {
        let input = "{\"ts\":100,\"level\":\"INFO\"}\n{\"ts\":101,\"level\":\"WARN\"}\n{\"ts\":102,\"level\":\"INFO\"}\n";
        let mut encoded = encode_jsonl_stream(
            Cursor::new(input),
            Vec::new(),
            schema(),
            BlockOptions {
                max_rows_per_block: 2,
                max_uncompressed_bytes_per_block: 1024,
            },
        )
        .unwrap();
        let inspect = inspect_stream(Cursor::new(&encoded)).unwrap();
        let second = &inspect.blocks[1];
        let corrupt_at = second.encoded_offset as usize + second.compressed_size as usize - 1;

        encoded[corrupt_at] ^= 0xff;

        let mut output = Vec::new();
        let err = decode_jsonl_stream(Cursor::new(encoded), &mut output, schema()).unwrap_err();

        assert_eq!(
            String::from_utf8(output).unwrap(),
            "{\"ts\":100,\"level\":\"INFO\"}\n{\"ts\":101,\"level\":\"WARN\"}\n"
        );
        assert!(matches!(
            err,
            crate::CompactError::InvalidInput("frame checksum mismatch")
        ));
    }
}
