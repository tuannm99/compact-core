//! Rolling append stream segmentation.
//!
//! Segments are cut only on valid block boundaries. This keeps every completed
//! segment independently recoverable and replayable.

use std::io::BufRead;

use crate::schema::Schema;
use crate::streaming::{BlockOptions, JsonlAppendWriter, read_bounded_jsonl_line};
use crate::{CompactError, Result};

/// Limits for one rolling append segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RollingOptions {
    pub max_segment_bytes: usize,
    pub max_blocks_per_segment: usize,
}

impl RollingOptions {
    pub fn validate(self) -> Result<Self> {
        if self.max_segment_bytes == 0 {
            return Err(CompactError::InvalidInput(
                "max segment bytes must be greater than zero",
            ));
        }

        if self.max_blocks_per_segment == 0 {
            return Err(CompactError::InvalidInput(
                "max blocks per segment must be greater than zero",
            ));
        }

        Ok(self)
    }
}

impl Default for RollingOptions {
    fn default() -> Self {
        Self {
            max_segment_bytes: 64 * 1024 * 1024,
            max_blocks_per_segment: 1024,
        }
    }
}

/// Encode JSONL into append segments using block-boundary rolling.
pub fn roll_jsonl_append_segments<R: BufRead>(
    mut input: R,
    schema: Schema,
    block_options: BlockOptions,
    rolling_options: RollingOptions,
) -> Result<Vec<Vec<u8>>> {
    let block_options = block_options.validate()?;
    let rolling_options = rolling_options.validate()?;
    let mut segments = Vec::new();
    let mut current: Option<JsonlAppendWriter<Vec<u8>>> = None;

    while let Some(line) =
        read_bounded_jsonl_line(&mut input, block_options.max_uncompressed_bytes_per_block)?
    {
        if line.trim().is_empty() {
            continue;
        }

        if current.as_ref().is_some_and(|writer| {
            writer.metadata().len() >= rolling_options.max_blocks_per_segment
                || writer.bytes_written() >= rolling_options.max_segment_bytes as u64
        }) {
            segments.push(current.take().expect("writer is present").finish()?);
        }

        let writer = match current.as_mut() {
            Some(writer) => writer,
            None => current.insert(JsonlAppendWriter::new(
                Vec::new(),
                schema.clone(),
                block_options,
            )?),
        };
        writer.write_jsonl_line(&line)?;
    }

    if let Some(writer) = current {
        segments.push(writer.finish()?);
    }

    Ok(segments)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{RollingOptions, roll_jsonl_append_segments};
    use crate::streaming::{BlockOptions, recover_append_stream, replay_jsonl_append_stream};

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
    fn rolling_segments_replay_all_rows() {
        let input = "{\"ts\":100}\n{\"ts\":101}\n{\"ts\":102}\n";
        let segments = roll_jsonl_append_segments(
            Cursor::new(input),
            schema(),
            BlockOptions {
                max_rows_per_block: 1,
                max_uncompressed_bytes_per_block: 1024,
            },
            RollingOptions {
                max_segment_bytes: usize::MAX,
                max_blocks_per_segment: 2,
            },
        )
        .unwrap();

        assert_eq!(segments.len(), 2);
        let mut decoded = Vec::new();
        for segment in &segments {
            let recovery = recover_append_stream(segment).unwrap();
            assert_eq!(recovery.valid_len as usize, segment.len());
            decoded.extend(replay_jsonl_append_stream(segment, Vec::new(), schema()).unwrap());
        }

        assert_eq!(String::from_utf8(decoded).unwrap(), input);
    }
}
