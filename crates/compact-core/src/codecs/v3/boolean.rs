//! CMP3 column payload codecs.
//!
//! Phase 3 implements boolean columns and the nullable-value contract. The
//! outer CMP3 row-group writer is intentionally separate: this module accepts
//! typed JSON rows, produces validated column metadata plus payload bytes, and
//! can decode that payload without relying on file-level framing.

use serde_json::{Map, Value};

use crate::format::v3::ColumnChunkMetadata;
use crate::primitives::bitmap;
use crate::schema::{ColumnSchema, SchemaCodec, SchemaValueType};
use crate::{CompactError, Result};

use super::EncodedColumnChunk;

const BOOLEAN_CODEC_METADATA_LEN: usize = 8;

/// Encode a schema-defined boolean column from JSON object rows.
///
/// Required fields must exist and contain JSON booleans. For nullable fields,
/// both a missing key and explicit JSON `null` represent a null value. The
/// value bitmap contains only non-null booleans; nullable row positions are
/// represented by a separate validity bitmap where `1` means non-null.
pub fn encode_boolean_column(
    column: &ColumnSchema,
    rows: &[Map<String, Value>],
) -> Result<EncodedColumnChunk> {
    validate_boolean_column(column)?;

    let mut validity = Vec::with_capacity(rows.len());
    let mut values = Vec::with_capacity(rows.len());

    for row in rows {
        match row.get(&column.name) {
            Some(Value::Bool(value)) => {
                validity.push(true);
                values.push(*value);
            }
            Some(Value::Null) | None if column.nullable => validity.push(false),
            Some(Value::Null) => {
                return Err(CompactError::InvalidInput(
                    "required boolean column must not be null",
                ));
            }
            None => {
                return Err(CompactError::InvalidInput(
                    "jsonl row missing required boolean column",
                ));
            }
            Some(_) => {
                return Err(CompactError::InvalidInput(
                    "jsonl boolean column must be bool or null",
                ));
            }
        }
    }

    let null_count = validity.iter().filter(|&&present| !present).count();
    let mut payload = Vec::new();
    if column.nullable {
        payload.extend_from_slice(&bitmap::encode(&validity));
    }
    payload.extend_from_slice(&bitmap::encode(&values));

    let value_count = u64::try_from(rows.len())
        .map_err(|_| CompactError::InvalidInput("cmp3 boolean row count is too large"))?;
    let null_count = u64::try_from(null_count)
        .map_err(|_| CompactError::InvalidInput("cmp3 boolean null count is too large"))?;
    let non_null_count = value_count
        .checked_sub(null_count)
        .ok_or(CompactError::InvalidInput("cmp3 boolean count underflow"))?;
    let compressed_size = u64::try_from(payload.len())
        .map_err(|_| CompactError::InvalidInput("cmp3 boolean payload is too large"))?;

    Ok(EncodedColumnChunk {
        metadata: ColumnChunkMetadata {
            name: column.name.clone(),
            value_type: SchemaValueType::Bool,
            nullable: column.nullable,
            codec: SchemaCodec::Bitmap,
            value_count,
            null_count,
            // A logical JSON boolean contributes one raw byte for comparison
            // purposes; nulls do not contribute value bytes.
            raw_size: non_null_count,
            compressed_size,
            codec_metadata: non_null_count.to_le_bytes().to_vec(),
            statistics_metadata: crate::statistics::encode_bool(
                values.iter().filter(|&&value| value).count() as u64,
            ),
        },
        payload,
    })
}

/// Decode a CMP3 boolean payload into logical row values.
///
/// The returned vector always has `metadata.value_count` entries. Null entries
/// are restored from the validity bitmap; non-null booleans are consumed from
/// the value bitmap in row order.
pub fn decode_boolean_column(
    metadata: &ColumnChunkMetadata,
    payload: &[u8],
) -> Result<Vec<Option<bool>>> {
    validate_boolean_metadata(metadata, payload)?;

    let value_count = usize::try_from(metadata.value_count)
        .map_err(|_| CompactError::InvalidInput("cmp3 boolean value count is too large"))?;
    let null_count = usize::try_from(metadata.null_count)
        .map_err(|_| CompactError::InvalidInput("cmp3 boolean null count is too large"))?;
    let non_null_count = value_count
        .checked_sub(null_count)
        .ok_or(CompactError::InvalidInput("cmp3 boolean count underflow"))?;
    let validity_len = if metadata.nullable {
        value_count.div_ceil(8)
    } else {
        0
    };
    if validity_len > payload.len() {
        return Err(CompactError::InvalidInput(
            "cmp3 boolean validity bitmap is truncated",
        ));
    }
    let (validity_payload, value_payload) = payload.split_at(validity_len);

    let validity = if metadata.nullable {
        bitmap::decode(validity_payload, value_count)?
    } else {
        vec![true; value_count]
    };
    let actual_non_null_count = validity.iter().filter(|&&present| present).count();
    if actual_non_null_count != non_null_count {
        return Err(CompactError::InvalidInput(
            "cmp3 validity bitmap does not match null count",
        ));
    }

    let values = bitmap::decode(value_payload, non_null_count)?;
    let mut value_iter = values.into_iter();
    let mut decoded = Vec::with_capacity(value_count);

    for present in validity {
        if present {
            decoded.push(Some(value_iter.next().ok_or(
                CompactError::InvalidInput("cmp3 boolean value bitmap is missing values"),
            )?));
        } else {
            decoded.push(None);
        }
    }

    if value_iter.next().is_some() {
        return Err(CompactError::InvalidInput(
            "cmp3 boolean value bitmap has extra values",
        ));
    }

    Ok(decoded)
}

fn validate_boolean_column(column: &ColumnSchema) -> Result<()> {
    if column.value_type != SchemaValueType::Bool || column.codec != SchemaCodec::Bitmap {
        return Err(CompactError::Unsupported(
            "cmp3 boolean encoder requires bool bitmap column",
        ));
    }

    Ok(())
}

fn validate_boolean_metadata(metadata: &ColumnChunkMetadata, payload: &[u8]) -> Result<()> {
    if metadata.value_type != SchemaValueType::Bool || metadata.codec != SchemaCodec::Bitmap {
        return Err(CompactError::InvalidInput(
            "cmp3 boolean metadata has incompatible type or codec",
        ));
    }
    if metadata.null_count > metadata.value_count {
        return Err(CompactError::InvalidInput(
            "cmp3 null count exceeds value count",
        ));
    }
    if !metadata.nullable && metadata.null_count != 0 {
        return Err(CompactError::InvalidInput(
            "cmp3 required column has null values",
        ));
    }
    if metadata.codec_metadata.len() != BOOLEAN_CODEC_METADATA_LEN {
        return Err(CompactError::InvalidInput(
            "cmp3 boolean codec metadata must contain non-null count",
        ));
    }

    let declared_non_null_count = u64::from_le_bytes(
        metadata
            .codec_metadata
            .as_slice()
            .try_into()
            .expect("validated boolean codec metadata length"),
    );
    let expected_non_null_count = metadata.value_count - metadata.null_count;
    if declared_non_null_count != expected_non_null_count {
        return Err(CompactError::InvalidInput(
            "cmp3 boolean codec metadata count mismatch",
        ));
    }

    let payload_len = u64::try_from(payload.len())
        .map_err(|_| CompactError::InvalidInput("cmp3 boolean payload is too large"))?;
    if metadata.compressed_size != payload_len {
        return Err(CompactError::InvalidInput(
            "cmp3 boolean payload size does not match metadata",
        ));
    }

    let value_count = usize::try_from(metadata.value_count)
        .map_err(|_| CompactError::InvalidInput("cmp3 boolean value count is too large"))?;
    let non_null_count = usize::try_from(expected_non_null_count)
        .map_err(|_| CompactError::InvalidInput("cmp3 boolean value count is too large"))?;
    let expected_payload_len = if metadata.nullable {
        value_count.div_ceil(8)
    } else {
        0
    }
    .checked_add(non_null_count.div_ceil(8))
    .ok_or(CompactError::InvalidInput(
        "cmp3 boolean payload length overflow",
    ))?;
    if payload.len() != expected_payload_len {
        return Err(CompactError::InvalidInput(
            "cmp3 boolean payload length does not match counts",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::{Map, Value, json};

    use super::{decode_boolean_column, encode_boolean_column};
    use crate::CompactError;
    use crate::schema::{ColumnSchema, SchemaCodec, SchemaValueType};

    fn column(nullable: bool) -> ColumnSchema {
        ColumnSchema {
            name: "active".to_owned(),
            value_type: SchemaValueType::Bool,
            codec: SchemaCodec::Bitmap,
            nullable,
        }
    }

    fn rows(values: &[Value]) -> Vec<Map<String, Value>> {
        values
            .iter()
            .map(|value| {
                let mut row = Map::new();
                if !value.is_array() {
                    row.insert("active".to_owned(), value.clone());
                }
                row
            })
            .collect()
    }

    #[test]
    fn required_boolean_column_roundtrips() {
        let input = rows(&[json!(true), json!(false), json!(true)]);
        let encoded = encode_boolean_column(&column(false), &input).unwrap();
        let decoded = decode_boolean_column(&encoded.metadata, &encoded.payload).unwrap();

        assert_eq!(decoded, vec![Some(true), Some(false), Some(true)]);
        assert_eq!(encoded.metadata.null_count, 0);
        assert!(!encoded.metadata.nullable);
    }

    #[test]
    fn nullable_mixed_boolean_column_roundtrips() {
        let input = rows(&[
            Value::Null,
            json!(true),
            json!([]),
            json!(false),
            Value::Null,
        ]);
        let encoded = encode_boolean_column(&column(true), &input).unwrap();
        let decoded = decode_boolean_column(&encoded.metadata, &encoded.payload).unwrap();

        assert_eq!(decoded, vec![None, Some(true), None, Some(false), None]);
        assert_eq!(encoded.metadata.value_count, 5);
        assert_eq!(encoded.metadata.null_count, 3);
    }

    #[test]
    fn nullable_boolean_column_without_nulls_roundtrips() {
        let input = rows(&[json!(false), json!(true), json!(false)]);
        let encoded = encode_boolean_column(&column(true), &input).unwrap();
        let decoded = decode_boolean_column(&encoded.metadata, &encoded.payload).unwrap();

        assert_eq!(decoded, vec![Some(false), Some(true), Some(false)]);
        assert_eq!(encoded.metadata.null_count, 0);
        // Nullable columns retain a validity bitmap even when every value is
        // present, so each block remains independently self-describing.
        assert_eq!(encoded.payload, vec![0b0000_0111, 0b0000_0010]);
    }

    #[test]
    fn nullable_all_null_column_roundtrips() {
        let input = rows(&[Value::Null, json!([]), Value::Null]);
        let encoded = encode_boolean_column(&column(true), &input).unwrap();
        let decoded = decode_boolean_column(&encoded.metadata, &encoded.payload).unwrap();

        assert_eq!(decoded, vec![None, None, None]);
        assert_eq!(encoded.payload, vec![0]);
        assert_eq!(encoded.metadata.codec_metadata, 0u64.to_le_bytes());
    }

    #[test]
    fn empty_boolean_column_roundtrips() {
        let encoded = encode_boolean_column(&column(true), &[]).unwrap();
        let decoded = decode_boolean_column(&encoded.metadata, &encoded.payload).unwrap();

        assert!(decoded.is_empty());
        assert!(encoded.payload.is_empty());
    }

    #[test]
    fn required_boolean_rejects_missing_and_null_fields() {
        for value in [json!([]), Value::Null] {
            let err = encode_boolean_column(&column(false), &rows(&[value])).unwrap_err();

            assert!(matches!(
                err,
                CompactError::InvalidInput(
                    "jsonl row missing required boolean column"
                        | "required boolean column must not be null"
                )
            ));
        }
    }

    #[test]
    fn boolean_column_rejects_wrong_json_type() {
        let err = encode_boolean_column(&column(true), &rows(&[json!("true")])).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("jsonl boolean column must be bool or null")
        ));
    }

    #[test]
    fn decoder_rejects_validity_count_mismatch() {
        let input = rows(&[Value::Null, json!(true)]);
        let mut encoded = encode_boolean_column(&column(true), &input).unwrap();
        encoded.payload[0] = 0b0000_0011;
        let err = decode_boolean_column(&encoded.metadata, &encoded.payload).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("cmp3 validity bitmap does not match null count")
        ));
    }

    #[test]
    fn decoder_rejects_non_zero_bitmap_padding() {
        let input = rows(&[json!(true)]);
        let mut encoded = encode_boolean_column(&column(false), &input).unwrap();
        encoded.payload[0] |= 0b1000_0000;
        let err = decode_boolean_column(&encoded.metadata, &encoded.payload).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("bitmap padding bits must be zero")
        ));
    }

    #[test]
    fn decoder_rejects_payload_and_codec_metadata_mismatches() {
        let input = rows(&[json!(true), Value::Null]);
        let encoded = encode_boolean_column(&column(true), &input).unwrap();

        let mut wrong_size = encoded.metadata.clone();
        wrong_size.compressed_size += 1;
        let err = decode_boolean_column(&wrong_size, &encoded.payload).unwrap_err();
        assert!(matches!(
            err,
            CompactError::InvalidInput("cmp3 boolean payload size does not match metadata")
        ));

        let mut wrong_count = encoded.metadata;
        wrong_count.codec_metadata = 2u64.to_le_bytes().to_vec();
        let err = decode_boolean_column(&wrong_count, &encoded.payload).unwrap_err();
        assert!(matches!(
            err,
            CompactError::InvalidInput("cmp3 boolean codec metadata count mismatch")
        ));
    }

    #[test]
    fn boolean_column_metadata_and_payload_roundtrip_independently() {
        let input = rows(&[json!(true), Value::Null, json!(false)]);
        let encoded = encode_boolean_column(&column(true), &input).unwrap();
        let metadata_bytes = crate::format::v3::encode_column_metadata(&encoded.metadata).unwrap();
        let (decoded_metadata, consumed) =
            crate::format::v3::decode_column_metadata(&metadata_bytes).unwrap();
        let decoded = decode_boolean_column(&decoded_metadata, &encoded.payload).unwrap();

        assert_eq!(consumed, metadata_bytes.len());
        assert_eq!(decoded, vec![Some(true), None, Some(false)]);
    }
}
