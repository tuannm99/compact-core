//! CMP3 UTF-8 string column codecs.

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use super::EncodedColumnChunk;
use crate::format::v3::ColumnChunkMetadata;
use crate::primitives::{bitmap, rle, varint};
use crate::schema::{ColumnSchema, SchemaCodec, SchemaValueType};
use crate::{CompactError, Result};

const COUNT_METADATA_LEN: usize = 8;
const MAX_DICTIONARY_ENTRIES: usize = 4096;

pub fn encode_string_column(
    column: &ColumnSchema,
    rows: &[Map<String, Value>],
) -> Result<EncodedColumnChunk> {
    validate_column(column)?;
    let (validity, values) = collect_values(column, rows)?;
    let null_count = validity.iter().filter(|&&present| !present).count();
    let encoded_values = match column.codec {
        SchemaCodec::Prefix => encode_prefix(&values)?,
        SchemaCodec::Dictionary => encode_dictionary(&values)?,
        SchemaCodec::Rle => rle::encode_rle(&encode_stored(&values)?),
        SchemaCodec::Stored => encode_stored(&values)?,
        _ => {
            return Err(CompactError::Unsupported(
                "cmp3 string encoder requires implemented string codec",
            ));
        }
    };
    let mut payload = Vec::new();
    if column.nullable {
        payload.extend_from_slice(&bitmap::encode(&validity));
    }
    payload.extend_from_slice(&encoded_values);
    let distinct_count = values
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>()
        .len();
    let raw_size = values.iter().try_fold(0u64, |total, value| {
        total
            .checked_add(value.len() as u64)
            .ok_or(CompactError::InvalidInput("cmp3 string raw size overflow"))
    })?;

    Ok(EncodedColumnChunk {
        metadata: ColumnChunkMetadata {
            name: column.name.clone(),
            value_type: SchemaValueType::String,
            nullable: column.nullable,
            codec: column.codec,
            value_count: count_u64(rows.len())?,
            null_count: count_u64(null_count)?,
            raw_size,
            compressed_size: count_u64(payload.len())?,
            codec_metadata: count_u64(values.len())?.to_le_bytes().to_vec(),
            statistics_metadata: crate::statistics::encode_string(count_u64(distinct_count)?),
        },
        payload,
    })
}

pub fn decode_string_column(
    metadata: &ColumnChunkMetadata,
    payload: &[u8],
) -> Result<Vec<Option<String>>> {
    validate_metadata(metadata, payload)?;
    let value_count = usize::try_from(metadata.value_count)
        .map_err(|_| CompactError::InvalidInput("cmp3 string value count is too large"))?;
    let non_null_count = usize::try_from(metadata.value_count - metadata.null_count)
        .map_err(|_| CompactError::InvalidInput("cmp3 string value count is too large"))?;
    let validity_len = if metadata.nullable {
        value_count.div_ceil(8)
    } else {
        0
    };
    if validity_len > payload.len() {
        return Err(CompactError::InvalidInput(
            "cmp3 string validity bitmap is truncated",
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
        SchemaCodec::Prefix => decode_prefix(values_payload, non_null_count)?,
        SchemaCodec::Dictionary => decode_dictionary(values_payload, non_null_count)?,
        SchemaCodec::Rle => decode_stored(&rle::decode_rle(values_payload)?, non_null_count)?,
        SchemaCodec::Stored => decode_stored(values_payload, non_null_count)?,
        _ => {
            return Err(CompactError::InvalidInput(
                "cmp3 string metadata has incompatible codec",
            ));
        }
    };
    let mut values = values.into_iter();
    validity
        .into_iter()
        .map(|present| {
            if present {
                values.next().map(Some).ok_or(CompactError::InvalidInput(
                    "cmp3 string payload is missing values",
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
) -> Result<(Vec<bool>, Vec<String>)> {
    let mut validity = Vec::with_capacity(rows.len());
    let mut values = Vec::with_capacity(rows.len());
    for row in rows {
        match row.get(&column.name) {
            Some(Value::String(value)) => {
                validity.push(true);
                values.push(value.clone());
            }
            Some(Value::Null) | None if column.nullable => validity.push(false),
            Some(Value::Null) => {
                return Err(CompactError::InvalidInput(
                    "required string column must not be null",
                ));
            }
            None => {
                return Err(CompactError::InvalidInput(
                    "jsonl row missing required string column",
                ));
            }
            Some(_) => {
                return Err(CompactError::InvalidInput(
                    "jsonl string column must be string or null",
                ));
            }
        }
    }
    Ok((validity, values))
}

fn encode_stored(values: &[String]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for value in values {
        out.extend_from_slice(&varint::encode_u64(&[count_u64(value.len())?]));
        out.extend_from_slice(value.as_bytes());
    }
    Ok(out)
}

fn decode_stored(data: &[u8], count: usize) -> Result<Vec<String>> {
    let mut cursor = 0;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        let len = varint::read_u64(data, &mut cursor)?;
        let len = usize::try_from(len)
            .map_err(|_| CompactError::InvalidInput("cmp3 string length is too large"))?;
        let bytes = read_exact(data, &mut cursor, len)?;
        values.push(
            std::str::from_utf8(bytes)
                .map_err(|_| CompactError::InvalidInput("cmp3 string must be utf-8"))?
                .to_owned(),
        );
    }
    if cursor != data.len() {
        return Err(CompactError::InvalidInput(
            "cmp3 string payload has trailing bytes",
        ));
    }
    Ok(values)
}

fn encode_prefix(values: &[String]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut previous = &[][..];
    for value in values {
        let bytes = value.as_bytes();
        let prefix = previous
            .iter()
            .zip(bytes)
            .take_while(|(left, right)| left == right)
            .count();
        let suffix = &bytes[prefix..];
        out.extend_from_slice(&varint::encode_u64(&[
            count_u64(prefix)?,
            count_u64(suffix.len())?,
        ]));
        out.extend_from_slice(suffix);
        previous = bytes;
    }
    Ok(out)
}

fn decode_prefix(data: &[u8], count: usize) -> Result<Vec<String>> {
    let mut cursor = 0;
    let mut previous = Vec::new();
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        let prefix = usize::try_from(varint::read_u64(data, &mut cursor)?)
            .map_err(|_| CompactError::InvalidInput("cmp3 prefix length is too large"))?;
        let suffix_len = usize::try_from(varint::read_u64(data, &mut cursor)?)
            .map_err(|_| CompactError::InvalidInput("cmp3 suffix length is too large"))?;
        if prefix > previous.len() {
            return Err(CompactError::InvalidInput(
                "cmp3 prefix exceeds previous string",
            ));
        }
        let suffix = read_exact(data, &mut cursor, suffix_len)?;
        let mut current = previous[..prefix].to_vec();
        current.extend_from_slice(suffix);
        let value = std::str::from_utf8(&current)
            .map_err(|_| CompactError::InvalidInput("cmp3 string must be utf-8"))?
            .to_owned();
        previous = current;
        values.push(value);
    }
    if cursor != data.len() {
        return Err(CompactError::InvalidInput(
            "cmp3 prefix payload has trailing bytes",
        ));
    }
    Ok(values)
}

fn encode_dictionary(values: &[String]) -> Result<Vec<u8>> {
    let mut dictionary = Vec::<String>::new();
    let mut ids = Vec::with_capacity(values.len());
    let mut by_value = HashMap::<&str, u64>::new();
    for value in values {
        let id = if let Some(id) = by_value.get(value.as_str()) {
            *id
        } else {
            if dictionary.len() >= MAX_DICTIONARY_ENTRIES {
                return Err(CompactError::Unsupported(
                    "cmp3 dictionary cardinality limit exceeded",
                ));
            }
            let id = count_u64(dictionary.len())?;
            dictionary.push(value.clone());
            by_value.insert(value, id);
            id
        };
        ids.push(id);
    }
    let mut out = Vec::new();
    out.extend_from_slice(&varint::encode_u64(&[count_u64(dictionary.len())?]));
    out.extend_from_slice(&encode_stored(&dictionary)?);
    out.extend_from_slice(&varint::encode_u64(&ids));
    Ok(out)
}

fn decode_dictionary(data: &[u8], count: usize) -> Result<Vec<String>> {
    let mut cursor = 0;
    let dictionary_count = usize::try_from(varint::read_u64(data, &mut cursor)?)
        .map_err(|_| CompactError::InvalidInput("cmp3 dictionary is too large"))?;
    if dictionary_count > MAX_DICTIONARY_ENTRIES {
        return Err(CompactError::InvalidInput(
            "cmp3 dictionary cardinality limit exceeded",
        ));
    }
    let mut dictionary = Vec::with_capacity(dictionary_count);
    for _ in 0..dictionary_count {
        let len = usize::try_from(varint::read_u64(data, &mut cursor)?)
            .map_err(|_| CompactError::InvalidInput("cmp3 string length is too large"))?;
        let bytes = read_exact(data, &mut cursor, len)?;
        dictionary.push(
            std::str::from_utf8(bytes)
                .map_err(|_| CompactError::InvalidInput("cmp3 string must be utf-8"))?
                .to_owned(),
        );
    }
    let ids = varint::decode_u64(&data[cursor..])?;
    if ids.len() != count {
        return Err(CompactError::InvalidInput(
            "cmp3 dictionary id count mismatch",
        ));
    }
    ids.into_iter()
        .map(|id| {
            let index = usize::try_from(id)
                .map_err(|_| CompactError::InvalidInput("cmp3 dictionary id is too large"))?;
            dictionary
                .get(index)
                .cloned()
                .ok_or(CompactError::InvalidInput(
                    "cmp3 dictionary id out of range",
                ))
        })
        .collect()
}

fn validate_column(column: &ColumnSchema) -> Result<()> {
    if column.value_type != SchemaValueType::String
        || !matches!(
            column.codec,
            SchemaCodec::Prefix | SchemaCodec::Dictionary | SchemaCodec::Rle | SchemaCodec::Stored
        )
    {
        return Err(CompactError::Unsupported(
            "cmp3 string encoder requires implemented string codec",
        ));
    }
    Ok(())
}

fn validate_metadata(metadata: &ColumnChunkMetadata, payload: &[u8]) -> Result<()> {
    if metadata.value_type != SchemaValueType::String
        || !matches!(
            metadata.codec,
            SchemaCodec::Prefix | SchemaCodec::Dictionary | SchemaCodec::Rle | SchemaCodec::Stored
        )
    {
        return Err(CompactError::InvalidInput(
            "cmp3 string metadata has incompatible type or codec",
        ));
    }
    if metadata.codec_metadata.len() != COUNT_METADATA_LEN
        || metadata.null_count > metadata.value_count
        || (!metadata.nullable && metadata.null_count != 0)
    {
        return Err(CompactError::InvalidInput(
            "cmp3 string metadata is invalid",
        ));
    }
    let count = u64::from_le_bytes(
        metadata
            .codec_metadata
            .as_slice()
            .try_into()
            .expect("validated string count metadata"),
    );
    if count != metadata.value_count - metadata.null_count {
        return Err(CompactError::InvalidInput(
            "cmp3 string codec metadata count mismatch",
        ));
    }
    if metadata.compressed_size != count_u64(payload.len())? {
        return Err(CompactError::InvalidInput(
            "cmp3 string payload size does not match metadata",
        ));
    }
    Ok(())
}

fn read_exact<'a>(data: &'a [u8], cursor: &mut usize, len: usize) -> Result<&'a [u8]> {
    let end = cursor
        .checked_add(len)
        .ok_or(CompactError::InvalidInput("cmp3 string length overflow"))?;
    if end > data.len() {
        return Err(CompactError::InvalidInput(
            "cmp3 string payload is truncated",
        ));
    }
    let bytes = &data[*cursor..end];
    *cursor = end;
    Ok(bytes)
}

fn count_u64(value: usize) -> Result<u64> {
    u64::try_from(value).map_err(|_| CompactError::InvalidInput("cmp3 string size is too large"))
}

#[cfg(test)]
mod tests {
    use serde_json::{Map, Value, json};

    use super::{decode_string_column, encode_string_column};
    use crate::schema::{ColumnSchema, SchemaCodec, SchemaValueType};

    fn column(codec: SchemaCodec, nullable: bool) -> ColumnSchema {
        ColumnSchema {
            name: "value".into(),
            value_type: SchemaValueType::String,
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
    fn all_string_codecs_roundtrip_unicode_and_empty_values() {
        let input = rows(&[json!(""), json!("service/api"), json!("service/á")]);
        for codec in [
            SchemaCodec::Prefix,
            SchemaCodec::Dictionary,
            SchemaCodec::Rle,
            SchemaCodec::Stored,
        ] {
            let encoded = encode_string_column(&column(codec, false), &input).unwrap();
            assert_eq!(
                decode_string_column(&encoded.metadata, &encoded.payload).unwrap(),
                vec![
                    Some("".into()),
                    Some("service/api".into()),
                    Some("service/á".into())
                ]
            );
        }
    }

    #[test]
    fn nullable_prefix_roundtrips_missing_and_null() {
        let input = rows(&[json!("a"), Value::Null, json!([]), json!("ab")]);
        let encoded = encode_string_column(&column(SchemaCodec::Prefix, true), &input).unwrap();
        assert_eq!(
            decode_string_column(&encoded.metadata, &encoded.payload).unwrap(),
            vec![Some("a".into()), None, None, Some("ab".into())]
        );
    }

    #[test]
    fn prefix_compresses_shared_prefixes_better_than_stored() {
        let input = rows(&[
            json!("service/api/users"),
            json!("service/api/orders"),
            json!("service/api/health"),
        ]);
        let prefix = encode_string_column(&column(SchemaCodec::Prefix, false), &input).unwrap();
        let stored = encode_string_column(&column(SchemaCodec::Stored, false), &input).unwrap();
        assert!(prefix.payload.len() < stored.payload.len());
    }
}
