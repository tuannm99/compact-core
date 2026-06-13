//! CMP3 `u64` column codecs.
//!
//! Numeric payloads optionally begin with a validity bitmap, followed by the
//! encoded non-null values. Codec metadata stores every parameter required to
//! validate and decode the payload; the decoder never infers a codec.

use serde_json::{Map, Value};

use super::EncodedColumnChunk;
use crate::format::v3::ColumnChunkMetadata;
use crate::pipeline::delta_varint;
use crate::primitives::{bitmap, bitpack};
use crate::schema::{ColumnSchema, SchemaCodec, SchemaValueType};
use crate::{CompactError, Result};

const COUNT_METADATA_LEN: usize = 8;
const BITPACK_METADATA_LEN: usize = 1 + 8;
const DELTA_BITPACK_METADATA_LEN: usize = 1 + 8 + 8;

pub fn encode_u64_column(
    column: &ColumnSchema,
    rows: &[Map<String, Value>],
) -> Result<EncodedColumnChunk> {
    validate_column(column)?;
    let (validity, values) = collect_values(column, rows)?;
    let null_count = validity.iter().filter(|&&present| !present).count();

    let (codec_metadata, encoded_values) = match column.codec {
        SchemaCodec::Bitpack => {
            let width = required_width(values.iter().copied().max().unwrap_or(0));
            let mut metadata = vec![width];
            metadata.extend_from_slice(&count_u64(values.len())?.to_le_bytes());
            (metadata, bitpack::encode_u64(&values, width)?)
        }
        SchemaCodec::DeltaBitpack => encode_delta_bitpack(&values)?,
        SchemaCodec::DeltaVarintU64 => (
            count_u64(values.len())?.to_le_bytes().to_vec(),
            delta_varint::encode_u64(&values)?,
        ),
        SchemaCodec::Stored => {
            let mut payload = Vec::with_capacity(values.len().saturating_mul(8));
            for value in &values {
                payload.extend_from_slice(&value.to_le_bytes());
            }
            (count_u64(values.len())?.to_le_bytes().to_vec(), payload)
        }
        _ => {
            return Err(CompactError::Unsupported(
                "cmp3 numeric encoder requires implemented u64 codec",
            ));
        }
    };

    let mut payload = Vec::new();
    if column.nullable {
        payload.extend_from_slice(&bitmap::encode(&validity));
    }
    payload.extend_from_slice(&encoded_values);

    Ok(EncodedColumnChunk {
        metadata: ColumnChunkMetadata {
            name: column.name.clone(),
            value_type: SchemaValueType::U64,
            nullable: column.nullable,
            codec: column.codec,
            value_count: count_u64(rows.len())?,
            null_count: count_u64(null_count)?,
            raw_size: count_u64(values.len())?
                .checked_mul(8)
                .ok_or(CompactError::InvalidInput("cmp3 numeric raw size overflow"))?,
            compressed_size: count_u64(payload.len())?,
            codec_metadata,
            statistics_metadata: crate::statistics::encode_u64(
                values.iter().copied().min(),
                values.iter().copied().max(),
            ),
        },
        payload,
    })
}

pub fn decode_u64_column(
    metadata: &ColumnChunkMetadata,
    payload: &[u8],
) -> Result<Vec<Option<u64>>> {
    validate_metadata(metadata, payload)?;
    let value_count = usize_count(
        metadata.value_count,
        "cmp3 numeric value count is too large",
    )?;
    let null_count = usize_count(metadata.null_count, "cmp3 numeric null count is too large")?;
    let non_null_count = value_count
        .checked_sub(null_count)
        .ok_or(CompactError::InvalidInput("cmp3 numeric count underflow"))?;
    let validity_len = if metadata.nullable {
        value_count.div_ceil(8)
    } else {
        0
    };
    if validity_len > payload.len() {
        return Err(CompactError::InvalidInput(
            "cmp3 numeric validity bitmap is truncated",
        ));
    }
    let (validity_payload, values_payload) = payload.split_at(validity_len);
    let validity = if metadata.nullable {
        bitmap::decode(validity_payload, value_count)?
    } else {
        vec![true; value_count]
    };
    if validity.iter().filter(|&&present| present).count() != non_null_count {
        return Err(CompactError::InvalidInput(
            "cmp3 validity bitmap does not match null count",
        ));
    }

    let values = match metadata.codec {
        SchemaCodec::Bitpack => {
            let width = metadata.codec_metadata[0];
            bitpack::decode_u64(values_payload, width, non_null_count)?
        }
        SchemaCodec::DeltaBitpack => {
            decode_delta_bitpack(metadata, values_payload, non_null_count)?
        }
        SchemaCodec::DeltaVarintU64 => {
            let values = delta_varint::decode_u64(values_payload)?;
            if values.len() != non_null_count {
                return Err(CompactError::InvalidInput(
                    "cmp3 numeric decoded count mismatch",
                ));
            }
            values
        }
        SchemaCodec::Stored => decode_stored(values_payload, non_null_count)?,
        _ => {
            return Err(CompactError::InvalidInput(
                "cmp3 numeric metadata has incompatible codec",
            ));
        }
    };

    let mut values = values.into_iter();
    validity
        .into_iter()
        .map(|present| {
            if present {
                values.next().map(Some).ok_or(CompactError::InvalidInput(
                    "cmp3 numeric payload is missing values",
                ))
            } else {
                Ok(None)
            }
        })
        .collect()
}

fn collect_values(
    column: &ColumnSchema,
    rows: &[Map<String, Value>],
) -> Result<(Vec<bool>, Vec<u64>)> {
    let mut validity = Vec::with_capacity(rows.len());
    let mut values = Vec::with_capacity(rows.len());
    for row in rows {
        match row.get(&column.name) {
            Some(Value::Number(value)) if value.as_u64().is_some() => {
                validity.push(true);
                values.push(value.as_u64().expect("checked u64 number"));
            }
            Some(Value::Null) | None if column.nullable => validity.push(false),
            Some(Value::Null) => {
                return Err(CompactError::InvalidInput(
                    "required u64 column must not be null",
                ));
            }
            None => {
                return Err(CompactError::InvalidInput(
                    "jsonl row missing required u64 column",
                ));
            }
            Some(_) => {
                return Err(CompactError::InvalidInput(
                    "jsonl u64 column must be an unsigned integer or null",
                ));
            }
        }
    }
    Ok((validity, values))
}

fn encode_delta_bitpack(values: &[u64]) -> Result<(Vec<u8>, Vec<u8>)> {
    let base = values.first().copied().unwrap_or(0);
    let mut previous = base;
    let mut deltas = Vec::with_capacity(values.len());
    for &value in values {
        let delta = value
            .checked_sub(previous)
            .ok_or(CompactError::InvalidInput(
                "delta bitpack requires monotonic u64 values",
            ))?;
        deltas.push(delta);
        previous = value;
    }
    let width = required_width(deltas.iter().copied().max().unwrap_or(0));
    let mut metadata = vec![width];
    metadata.extend_from_slice(&base.to_le_bytes());
    metadata.extend_from_slice(&count_u64(values.len())?.to_le_bytes());
    Ok((metadata, bitpack::encode_u64(&deltas, width)?))
}

fn decode_delta_bitpack(
    metadata: &ColumnChunkMetadata,
    payload: &[u8],
    count: usize,
) -> Result<Vec<u64>> {
    let width = metadata.codec_metadata[0];
    let base = u64::from_le_bytes(
        metadata.codec_metadata[1..9]
            .try_into()
            .expect("validated delta bitpack base"),
    );
    let deltas = bitpack::decode_u64(payload, width, count)?;
    let mut previous = base;
    let mut values = Vec::with_capacity(count);
    for delta in deltas {
        let value = previous
            .checked_add(delta)
            .ok_or(CompactError::InvalidInput("delta bitpack decode overflow"))?;
        values.push(value);
        previous = value;
    }
    Ok(values)
}

fn decode_stored(payload: &[u8], count: usize) -> Result<Vec<u64>> {
    let expected = count
        .checked_mul(8)
        .ok_or(CompactError::InvalidInput("cmp3 stored length overflow"))?;
    if payload.len() != expected {
        return Err(CompactError::InvalidInput(
            "cmp3 stored payload length mismatch",
        ));
    }
    Ok(payload
        .chunks_exact(8)
        .map(|bytes| u64::from_le_bytes(bytes.try_into().expect("eight-byte chunk")))
        .collect())
}

fn validate_column(column: &ColumnSchema) -> Result<()> {
    if column.value_type != SchemaValueType::U64
        || !matches!(
            column.codec,
            SchemaCodec::Bitpack
                | SchemaCodec::DeltaBitpack
                | SchemaCodec::DeltaVarintU64
                | SchemaCodec::Stored
        )
    {
        return Err(CompactError::Unsupported(
            "cmp3 numeric encoder requires implemented u64 codec",
        ));
    }
    Ok(())
}

fn validate_metadata(metadata: &ColumnChunkMetadata, payload: &[u8]) -> Result<()> {
    if metadata.value_type != SchemaValueType::U64 {
        return Err(CompactError::InvalidInput(
            "cmp3 numeric metadata has incompatible type",
        ));
    }
    if metadata.null_count > metadata.value_count
        || (!metadata.nullable && metadata.null_count != 0)
    {
        return Err(CompactError::InvalidInput(
            "cmp3 numeric null count is invalid",
        ));
    }
    let expected_metadata_len = match metadata.codec {
        SchemaCodec::Bitpack => BITPACK_METADATA_LEN,
        SchemaCodec::DeltaBitpack => DELTA_BITPACK_METADATA_LEN,
        SchemaCodec::DeltaVarintU64 | SchemaCodec::Stored => COUNT_METADATA_LEN,
        _ => {
            return Err(CompactError::InvalidInput(
                "cmp3 numeric metadata has incompatible codec",
            ));
        }
    };
    if metadata.codec_metadata.len() != expected_metadata_len {
        return Err(CompactError::InvalidInput(
            "cmp3 numeric codec metadata length mismatch",
        ));
    }
    if matches!(
        metadata.codec,
        SchemaCodec::Bitpack | SchemaCodec::DeltaBitpack
    ) && metadata.codec_metadata[0] > 64
    {
        return Err(CompactError::InvalidInput("bit width must be <= 64"));
    }
    let declared_count_offset = expected_metadata_len - 8;
    let declared_count = u64::from_le_bytes(
        metadata.codec_metadata[declared_count_offset..]
            .try_into()
            .expect("validated numeric count metadata"),
    );
    if declared_count != metadata.value_count - metadata.null_count {
        return Err(CompactError::InvalidInput(
            "cmp3 numeric codec metadata count mismatch",
        ));
    }
    if metadata.compressed_size != count_u64(payload.len())? {
        return Err(CompactError::InvalidInput(
            "cmp3 numeric payload size does not match metadata",
        ));
    }
    Ok(())
}

fn required_width(value: u64) -> u8 {
    (u64::BITS - value.leading_zeros()) as u8
}

fn count_u64(value: usize) -> Result<u64> {
    u64::try_from(value).map_err(|_| CompactError::InvalidInput("cmp3 numeric size is too large"))
}

fn usize_count(value: u64, error: &'static str) -> Result<usize> {
    usize::try_from(value).map_err(|_| CompactError::InvalidInput(error))
}

#[cfg(test)]
mod tests {
    use serde_json::{Map, Value, json};

    use super::{decode_u64_column, encode_u64_column};
    use crate::CompactError;
    use crate::schema::{ColumnSchema, SchemaCodec, SchemaValueType};

    fn column(codec: SchemaCodec, nullable: bool) -> ColumnSchema {
        ColumnSchema {
            name: "value".into(),
            value_type: SchemaValueType::U64,
            codec,
            nullable,
        }
    }

    fn rows(values: &[Value]) -> Vec<Map<String, Value>> {
        values
            .iter()
            .map(|value| {
                let mut row = Map::new();
                if !value.is_array() {
                    row.insert("value".into(), value.clone());
                }
                row
            })
            .collect()
    }

    #[test]
    fn bitpack_widths_zero_through_sixty_four_roundtrip() {
        for width in 0..=64 {
            let value = if width == 0 {
                0
            } else if width == 64 {
                u64::MAX
            } else {
                (1u64 << width) - 1
            };
            let encoded =
                encode_u64_column(&column(SchemaCodec::Bitpack, false), &rows(&[json!(value)]))
                    .unwrap();
            assert_eq!(encoded.metadata.codec_metadata[0], width);
            assert_eq!(
                decode_u64_column(&encoded.metadata, &encoded.payload).unwrap(),
                vec![Some(value)]
            );
        }
    }

    #[test]
    fn delta_bitpack_small_deltas_roundtrip_smaller_than_stored() {
        let input = rows(&[json!(1000), json!(1001), json!(1002), json!(1003)]);
        let packed = encode_u64_column(&column(SchemaCodec::DeltaBitpack, false), &input).unwrap();
        let stored = encode_u64_column(&column(SchemaCodec::Stored, false), &input).unwrap();

        assert!(packed.payload.len() < stored.payload.len());
        assert_eq!(
            decode_u64_column(&packed.metadata, &packed.payload).unwrap(),
            vec![Some(1000), Some(1001), Some(1002), Some(1003)]
        );
    }

    #[test]
    fn delta_bitpack_small_deltas_are_smaller_than_delta_varint() {
        let input = rows(&(1000..1064).map(|value| json!(value)).collect::<Vec<_>>());
        let bitpacked =
            encode_u64_column(&column(SchemaCodec::DeltaBitpack, false), &input).unwrap();
        let varint =
            encode_u64_column(&column(SchemaCodec::DeltaVarintU64, false), &input).unwrap();

        assert!(bitpacked.payload.len() < varint.payload.len());
    }

    #[test]
    fn nullable_stored_roundtrips_missing_and_null_values() {
        let input = rows(&[json!(1), Value::Null, json!([]), json!(u64::MAX)]);
        let encoded = encode_u64_column(&column(SchemaCodec::Stored, true), &input).unwrap();

        assert_eq!(
            decode_u64_column(&encoded.metadata, &encoded.payload).unwrap(),
            vec![Some(1), None, None, Some(u64::MAX)]
        );
    }

    #[test]
    fn delta_bitpack_rejects_decreasing_values() {
        let err = encode_u64_column(
            &column(SchemaCodec::DeltaBitpack, false),
            &rows(&[json!(2), json!(1)]),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("delta bitpack requires monotonic u64 values")
        ));
    }

    #[test]
    fn decoder_rejects_invalid_width_and_count_metadata() {
        let input = rows(&[json!(1)]);
        let encoded = encode_u64_column(&column(SchemaCodec::Bitpack, false), &input).unwrap();

        let mut invalid_width = encoded.metadata.clone();
        invalid_width.codec_metadata[0] = 65;
        assert!(matches!(
            decode_u64_column(&invalid_width, &encoded.payload).unwrap_err(),
            CompactError::InvalidInput("bit width must be <= 64")
        ));

        let mut invalid_count = encoded.metadata;
        invalid_count.codec_metadata[1..9].copy_from_slice(&2u64.to_le_bytes());
        assert!(matches!(
            decode_u64_column(&invalid_count, &encoded.payload).unwrap_err(),
            CompactError::InvalidInput("cmp3 numeric codec metadata count mismatch")
        ));
    }

    #[test]
    fn decoder_rejects_truncated_validity_without_panicking() {
        let input = rows(&[json!(1)]);
        let encoded = encode_u64_column(&column(SchemaCodec::Bitpack, true), &input).unwrap();
        let mut metadata = encoded.metadata;
        metadata.value_count = 80;
        metadata.null_count = 79;
        metadata.compressed_size = encoded.payload.len() as u64;
        let err = decode_u64_column(&metadata, &encoded.payload).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("cmp3 numeric validity bitmap is truncated")
        ));
    }
}
