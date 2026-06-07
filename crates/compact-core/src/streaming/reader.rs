//! Streaming JSONL block reader.
//!
//! This module will own sequential decode for v0.2 files. The first reader
//! should validate one block at a time and stop on the first corrupted block so
//! decode never returns mixed trusted and untrusted output.

use std::io::{ErrorKind, Read};

use crate::io::decode_jsonl;
use crate::primitives::crc32;
use crate::schema::Schema;
use crate::streaming::{BLOCK_MAGIC_V1, BlockMetadata};
use crate::{Codec, CompactError, MAGIC_V1, MAGIC_V2, Result, VERSION_V2, framing};

const STREAM_HEADER_LEN: usize = 4 + 1 + 1 + 4;
const FRAME_HEADER_LEN: usize = 4 + 1 + 1 + 8 + 4;
const FRAME_PAYLOAD_LEN_OFFSET: usize = 4 + 1 + 1;

/// One decoded JSONL block from a v0.2 stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedBlock {
    pub metadata: BlockMetadata,
    pub first_row_index: u64,
    pub jsonl: String,
}

/// Metadata summary for a v0.2 stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamInspect {
    pub blocks: Vec<BlockMetadata>,
    pub total_rows: u64,
    pub total_uncompressed_size: u64,
    pub total_compressed_size: u64,
}

/// Sequential reader for v0.2 JSONL block streams.
///
/// The reader validates the file header once, then reads one framed block at a
/// time. It returns `Ok(None)` only at a clean block boundary EOF. Truncated
/// headers, truncated payloads, invalid checksums, and malformed block metadata
/// are reported as errors.
#[derive(Debug)]
pub struct JsonlBlockReader<R: Read> {
    reader: R,
    schema: Schema,
    next_block_index: u64,
    next_row_index: u64,
    next_offset: u64,
}

impl<R: Read> JsonlBlockReader<R> {
    /// Create a reader and validate the v0.2 stream header.
    pub fn new(mut reader: R, schema: Schema) -> Result<Self> {
        validate_stream_header(&mut reader)?;

        Ok(Self {
            reader,
            schema,
            next_block_index: 0,
            next_row_index: 0,
            next_offset: STREAM_HEADER_LEN as u64,
        })
    }

    /// Decode the next block, or return `Ok(None)` at clean EOF.
    pub fn next_block(&mut self) -> Result<Option<DecodedBlock>> {
        let Some(frame) = self.read_next_frame()? else {
            return Ok(None);
        };

        let frame_len = frame.len();
        let decoded = framing::decode_v1(&frame)?;
        if decoded.codec != Codec::ColumnBlock {
            return Err(CompactError::InvalidInput(
                "stream block frame must use column block codec",
            ));
        }

        let parsed = parse_block_payload(&decoded.payload)?;
        if parsed.block_index != self.next_block_index {
            return Err(CompactError::InvalidInput(
                "stream block index is not sequential",
            ));
        }

        if parsed.first_row_index != self.next_row_index {
            return Err(CompactError::InvalidInput(
                "stream block first row index is not sequential",
            ));
        }

        let column_frame = framing::encode_v1(Codec::ColumnBlock, parsed.column_block);
        let jsonl = decode_jsonl(&column_frame, &self.schema)?;
        let decoded_rows = count_jsonl_rows(&jsonl);
        if decoded_rows != parsed.row_count {
            return Err(CompactError::InvalidInput(
                "decoded block row count does not match metadata",
            ));
        }

        if jsonl.len() != parsed.raw_size {
            return Err(CompactError::InvalidInput(
                "decoded block raw size does not match metadata",
            ));
        }

        let metadata = BlockMetadata {
            block_index: parsed.block_index,
            encoded_offset: self.next_offset,
            row_count: usize_to_u64(parsed.row_count, "row count is too large")?,
            uncompressed_size: usize_to_u64(parsed.raw_size, "row group is too large")?,
            compressed_size: usize_to_u64(frame_len, "block frame is too large")?,
            checksum: crc32::checksum(&decoded.payload),
        };
        self.next_block_index += 1;
        self.next_row_index += metadata.row_count;
        self.next_offset += metadata.compressed_size;

        Ok(Some(DecodedBlock {
            metadata,
            first_row_index: parsed.first_row_index,
            jsonl,
        }))
    }

    fn read_next_frame(&mut self) -> Result<Option<Vec<u8>>> {
        read_next_frame_from(&mut self.reader)
    }
}

/// Inspect v0.2 block metadata without decoding column values.
pub fn inspect_jsonl_stream<R: Read>(mut reader: R) -> Result<StreamInspect> {
    validate_stream_header(&mut reader)?;

    let mut blocks = Vec::new();
    let mut next_offset = STREAM_HEADER_LEN as u64;
    let mut expected_block_index = 0u64;
    let mut expected_first_row_index = 0u64;

    while let Some(frame) = read_next_frame_from(&mut reader)? {
        let frame_len = frame.len();
        let decoded = framing::decode_v1(&frame)?;
        if decoded.codec != Codec::ColumnBlock {
            return Err(CompactError::InvalidInput(
                "stream block frame must use column block codec",
            ));
        }

        let parsed = parse_block_payload(&decoded.payload)?;
        if parsed.block_index != expected_block_index {
            return Err(CompactError::InvalidInput(
                "stream block index is not sequential",
            ));
        }

        if parsed.first_row_index != expected_first_row_index {
            return Err(CompactError::InvalidInput(
                "stream block first row index is not sequential",
            ));
        }

        let metadata = BlockMetadata {
            block_index: parsed.block_index,
            encoded_offset: next_offset,
            row_count: usize_to_u64(parsed.row_count, "row count is too large")?,
            uncompressed_size: usize_to_u64(parsed.raw_size, "row group is too large")?,
            compressed_size: usize_to_u64(frame_len, "block frame is too large")?,
            checksum: crc32::checksum(&decoded.payload),
        };

        expected_block_index += 1;
        expected_first_row_index += metadata.row_count;
        next_offset += metadata.compressed_size;
        blocks.push(metadata);
    }

    let total_rows = blocks.iter().map(|block| block.row_count).sum();
    let total_uncompressed_size = blocks.iter().map(|block| block.uncompressed_size).sum();
    let total_compressed_size = blocks.iter().map(|block| block.compressed_size).sum();

    Ok(StreamInspect {
        blocks,
        total_rows,
        total_uncompressed_size,
        total_compressed_size,
    })
}

fn validate_stream_header<R: Read>(reader: &mut R) -> Result<()> {
    let mut header = [0u8; STREAM_HEADER_LEN];
    read_exact_invalid(reader, &mut header, "stream header is truncated")?;

    if header[0..4] != MAGIC_V2 {
        return Err(CompactError::InvalidInput("invalid stream magic"));
    }

    if header[4] != VERSION_V2 {
        return Err(CompactError::Unsupported("stream version"));
    }

    if header[5] != 0 {
        return Err(CompactError::Unsupported("stream flags"));
    }

    let extension_len = u32::from_le_bytes(
        header[6..10]
            .try_into()
            .expect("fixed stream header length is checked"),
    );
    if extension_len != 0 {
        return Err(CompactError::Unsupported("stream header extension"));
    }

    Ok(())
}

fn read_next_frame_from<R: Read>(reader: &mut R) -> Result<Option<Vec<u8>>> {
    let mut header = [0u8; FRAME_HEADER_LEN];
    match reader.read(&mut header[0..1]) {
        Ok(0) => return Ok(None),
        Ok(1) => {}
        Ok(_) => unreachable!("one-byte read cannot return more than one byte"),
        Err(err) => return Err(CompactError::Io(err)),
    }

    read_exact_invalid(reader, &mut header[1..], "frame header is truncated")?;

    if header[0..4] != MAGIC_V1 {
        return Err(CompactError::InvalidInput("invalid frame magic"));
    }

    let payload_len_start = FRAME_PAYLOAD_LEN_OFFSET;
    let payload_len_end = payload_len_start + 8;
    let payload_len = u64::from_le_bytes(
        header[payload_len_start..payload_len_end]
            .try_into()
            .expect("fixed frame header length is checked"),
    );
    let payload_len = usize::try_from(payload_len)
        .map_err(|_| CompactError::InvalidInput("frame payload length is too large"))?;
    let mut frame = Vec::with_capacity(FRAME_HEADER_LEN + payload_len);

    frame.extend_from_slice(&header);
    frame.resize(FRAME_HEADER_LEN + payload_len, 0);
    read_exact_invalid(
        reader,
        &mut frame[FRAME_HEADER_LEN..],
        "frame payload is truncated",
    )?;

    Ok(Some(frame))
}

struct ParsedBlockPayload<'a> {
    block_index: u64,
    first_row_index: u64,
    row_count: usize,
    raw_size: usize,
    column_block: &'a [u8],
}

fn parse_block_payload(payload: &[u8]) -> Result<ParsedBlockPayload<'_>> {
    let mut cursor = 0usize;
    let magic = read_slice(
        payload,
        &mut cursor,
        BLOCK_MAGIC_V1.len(),
        "block header is truncated",
    )?;
    if magic != BLOCK_MAGIC_V1 {
        return Err(CompactError::InvalidInput("invalid block magic"));
    }

    let block_index = read_u64(payload, &mut cursor, "block index is truncated")?;
    let first_row_index = read_u64(payload, &mut cursor, "block first row index is truncated")?;
    let row_count = u64_to_usize(
        read_u64(payload, &mut cursor, "block row count is truncated")?,
        "block row count is too large",
    )?;
    let raw_size = u64_to_usize(
        read_u64(payload, &mut cursor, "block raw size is truncated")?,
        "block raw size is too large",
    )?;
    let column_block_len = u64_to_usize(
        read_u64(
            payload,
            &mut cursor,
            "block column payload length is truncated",
        )?,
        "block column payload length is too large",
    )?;
    let column_block = read_slice(
        payload,
        &mut cursor,
        column_block_len,
        "block column payload is truncated",
    )?;

    if cursor != payload.len() {
        return Err(CompactError::InvalidInput(
            "block payload has trailing bytes",
        ));
    }

    Ok(ParsedBlockPayload {
        block_index,
        first_row_index,
        row_count,
        raw_size,
        column_block,
    })
}

fn read_exact_invalid<R: Read>(reader: &mut R, out: &mut [u8], err: &'static str) -> Result<()> {
    reader.read_exact(out).map_err(|io_err| {
        if io_err.kind() == ErrorKind::UnexpectedEof {
            CompactError::InvalidInput(err)
        } else {
            CompactError::Io(io_err)
        }
    })
}

fn read_slice<'a>(
    data: &'a [u8],
    cursor: &mut usize,
    len: usize,
    err: &'static str,
) -> Result<&'a [u8]> {
    let end = cursor
        .checked_add(len)
        .ok_or(CompactError::InvalidInput("block payload length overflow"))?;

    if end > data.len() {
        return Err(CompactError::InvalidInput(err));
    }

    let slice = &data[*cursor..end];
    *cursor = end;

    Ok(slice)
}

fn read_u64(data: &[u8], cursor: &mut usize, err: &'static str) -> Result<u64> {
    let bytes = read_slice(data, cursor, 8, err)?;

    Ok(u64::from_le_bytes(
        bytes
            .try_into()
            .expect("read_slice returned exactly eight bytes"),
    ))
}

fn count_jsonl_rows(jsonl: &str) -> usize {
    jsonl.lines().filter(|line| !line.trim().is_empty()).count()
}

fn u64_to_usize(value: u64, err: &'static str) -> Result<usize> {
    usize::try_from(value).map_err(|_| CompactError::InvalidInput(err))
}

fn usize_to_u64(value: usize, err: &'static str) -> Result<u64> {
    u64::try_from(value).map_err(|_| CompactError::InvalidInput(err))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::JsonlBlockReader;
    use crate::streaming::{BlockOptions, JsonlBlockWriter, inspect_stream};
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

    fn encode_stream(lines: &[&str], options: BlockOptions) -> Vec<u8> {
        let mut writer = JsonlBlockWriter::new(Vec::new(), schema(), options).unwrap();

        for line in lines {
            writer.write_jsonl_line(line).unwrap();
        }

        writer.finish().unwrap()
    }

    fn corrupt_second_block(mut data: Vec<u8>) -> Vec<u8> {
        let inspect = inspect_stream(Cursor::new(&data)).unwrap();
        let second = inspect
            .blocks
            .get(1)
            .expect("fixture should have two blocks");
        let corrupt_at = second.encoded_offset as usize + second.compressed_size as usize - 1;

        data[corrupt_at] ^= 0xff;

        data
    }

    #[test]
    fn empty_stream_returns_no_blocks() {
        let data = encode_stream(&[], BlockOptions::default());
        let mut reader = JsonlBlockReader::new(Cursor::new(data), schema()).unwrap();

        assert_eq!(reader.next_block().unwrap(), None);
    }

    #[test]
    fn one_block_roundtrip_is_byte_stable() {
        let data = encode_stream(&["{\"ts\":100}", "{\"ts\":101}"], BlockOptions::default());
        let mut reader = JsonlBlockReader::new(Cursor::new(data), schema()).unwrap();
        let block = reader.next_block().unwrap().unwrap();

        assert_eq!(block.first_row_index, 0);
        assert_eq!(block.metadata.block_index, 0);
        assert_eq!(block.metadata.row_count, 2);
        assert_eq!(block.jsonl, "{\"ts\":100}\n{\"ts\":101}\n");
        assert_eq!(reader.next_block().unwrap(), None);
    }

    #[test]
    fn multiple_blocks_roundtrip_sequentially() {
        let data = encode_stream(
            &["{\"ts\":100}", "{\"ts\":101}", "{\"ts\":102}"],
            BlockOptions {
                max_rows_per_block: 2,
                max_uncompressed_bytes_per_block: 1024,
            },
        );
        let mut reader = JsonlBlockReader::new(Cursor::new(data), schema()).unwrap();
        let first = reader.next_block().unwrap().unwrap();
        let second = reader.next_block().unwrap().unwrap();

        assert_eq!(first.first_row_index, 0);
        assert_eq!(first.metadata.row_count, 2);
        assert_eq!(first.jsonl, "{\"ts\":100}\n{\"ts\":101}\n");
        assert_eq!(second.first_row_index, 2);
        assert_eq!(second.metadata.row_count, 1);
        assert_eq!(second.jsonl, "{\"ts\":102}\n");
        assert_eq!(reader.next_block().unwrap(), None);
    }

    #[test]
    fn reader_rejects_invalid_stream_magic() {
        let mut data = encode_stream(&[], BlockOptions::default());
        data[0] = b'X';
        let err = JsonlBlockReader::new(Cursor::new(data), schema()).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("invalid stream magic")
        ));
    }

    #[test]
    fn reader_rejects_unsupported_stream_version() {
        let mut data = encode_stream(&[], BlockOptions::default());
        data[4] = VERSION_V2 + 1;
        let err = JsonlBlockReader::new(Cursor::new(data), schema()).unwrap_err();

        assert!(matches!(err, CompactError::Unsupported("stream version")));
    }

    #[test]
    fn reader_rejects_truncated_stream_header() {
        let err = JsonlBlockReader::new(Cursor::new(&MAGIC_V2[..]), schema()).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("stream header is truncated")
        ));
    }

    #[test]
    fn reader_rejects_truncated_frame_payload() {
        let mut data = encode_stream(&["{\"ts\":100}"], BlockOptions::default());
        data.pop();
        let mut reader = JsonlBlockReader::new(Cursor::new(data), schema()).unwrap();
        let err = reader.next_block().unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("frame payload is truncated")
        ));
    }

    #[test]
    fn reader_rejects_corrupted_block_checksum() {
        let mut data = encode_stream(&["{\"ts\":100}"], BlockOptions::default());
        let last = data.len() - 1;
        data[last] ^= 0xff;
        let mut reader = JsonlBlockReader::new(Cursor::new(data), schema()).unwrap();
        let err = reader.next_block().unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("frame checksum mismatch")
        ));
    }

    #[test]
    fn reader_isolates_corruption_to_later_block() {
        let data = encode_stream(
            &["{\"ts\":100}", "{\"ts\":101}", "{\"ts\":102}"],
            BlockOptions {
                max_rows_per_block: 2,
                max_uncompressed_bytes_per_block: 1024,
            },
        );
        let data = corrupt_second_block(data);
        let mut reader = JsonlBlockReader::new(Cursor::new(data), schema()).unwrap();
        let first = reader.next_block().unwrap().unwrap();
        let err = reader.next_block().unwrap_err();

        assert_eq!(first.metadata.block_index, 0);
        assert_eq!(first.jsonl, "{\"ts\":100}\n{\"ts\":101}\n");
        assert!(matches!(
            err,
            CompactError::InvalidInput("frame checksum mismatch")
        ));
    }
}
