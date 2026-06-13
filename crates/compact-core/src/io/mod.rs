//! JSONL conversion helpers for the first columnar MVP.
//!
//! JSONL is row-oriented, while the compression primitives work best on typed
//! columns. This module bridges that gap by collecting values per schema
//! column, encoding each column independently, and storing those column payloads
//! inside one outer frame.

pub mod v3;

use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::primitives::{rle, varint};
use crate::schema::{ColumnSchema, Schema, SchemaCodec, SchemaValueType};
use crate::{Codec, CompactError, Result, codecs, framing};

const COLUMN_BLOCK_MAGIC: [u8; 4] = *b"CBL1";
const COLUMN_COUNT_LEN: usize = 4;
const NAME_LEN_LEN: usize = 2;
const ROW_COUNT_LEN: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonlInspect {
    pub outer_codec: Codec,
    pub payload_len: usize,
    pub columns: Vec<ColumnInspect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnInspect {
    pub name: String,
    pub codec: SchemaCodec,
    pub row_count: usize,
    pub payload_len: usize,
}

/// Encoded JSONL row group before it is placed in a streaming v0.2 block.
///
/// A row group is the unit v0.2 will flush independently. It contains only the
/// column-block payload, not the outer file/block envelope. Keeping this type
/// separate lets the existing v0.1 one-shot encoder and the upcoming streaming
/// writer share the same columnar encoding logic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JsonlRowGroup {
    pub row_count: usize,
    pub raw_bytes: usize,
    pub payload: Vec<u8>,
}

/// Encode JSONL rows using required schema columns.
pub fn encode_jsonl(input: &str, schema: &Schema) -> Result<Vec<u8>> {
    let row_group = encode_jsonl_row_group(input, schema)?;

    Ok(framing::encode_v1(Codec::ColumnBlock, &row_group.payload))
}

/// Encode a bounded JSONL row group into the current column-block payload.
///
/// This is the extraction point for v0.2 streaming. The caller owns deciding
/// how many rows/bytes belong in the group; this function only validates those
/// rows against the schema and emits one independently decodable column block.
pub(crate) fn encode_jsonl_row_group(input: &str, schema: &Schema) -> Result<JsonlRowGroup> {
    let columns = schema.supported_columns()?;
    let mut column_values = columns
        .iter()
        .map(ColumnValues::new)
        .collect::<Result<Vec<_>>>()?;
    let mut row_count = 0usize;

    for line in input.lines().filter(|line| !line.trim().is_empty()) {
        let row = parse_jsonl_object(line)?;

        for (column_index, column) in columns.iter().enumerate() {
            let raw = row.get(&column.name).ok_or(CompactError::InvalidInput(
                "jsonl row missing schema column",
            ))?;
            column_values[column_index].push_json(raw)?;
        }

        row_count += 1;
    }

    let mut blocks = Vec::new();
    blocks.extend_from_slice(&COLUMN_BLOCK_MAGIC);
    let column_count = u32::try_from(columns.len())
        .map_err(|_| CompactError::InvalidInput("schema has too many columns"))?;
    blocks.extend_from_slice(&column_count.to_le_bytes());

    for (column, values) in columns.iter().zip(column_values.iter()) {
        let payload = values.encode(column)?;
        write_column_block(&mut blocks, column, row_count, &payload)?;
    }

    Ok(JsonlRowGroup {
        row_count,
        raw_bytes: input.len(),
        payload: blocks,
    })
}

/// Decode a column-block JSONL frame back into compact one-object-per-line JSONL.
pub fn decode_jsonl(frame: &[u8], schema: &Schema) -> Result<String> {
    let columns = schema.supported_columns()?;
    let outer = framing::decode_v1(frame)?;

    if outer.codec != Codec::ColumnBlock {
        return Err(CompactError::InvalidInput(
            "frame codec does not match jsonl schema",
        ));
    }

    let decoded_columns = decode_column_blocks(&outer.payload, columns)?;
    let row_count = decoded_columns
        .first()
        .map(|column| column.values.len())
        .unwrap_or(0);
    let mut out = String::new();

    for row_index in 0..row_count {
        out.push_str(&render_json_object(&decoded_columns, row_index)?);
        out.push('\n');
    }

    Ok(out)
}

pub fn inspect_jsonl(frame: &[u8]) -> Result<JsonlInspect> {
    let outer = framing::decode_v1(frame)?;

    if outer.codec != Codec::ColumnBlock {
        return Err(CompactError::InvalidInput(
            "frame is not a jsonl column block",
        ));
    }

    let columns = inspect_column_blocks(&outer.payload)?;

    Ok(JsonlInspect {
        outer_codec: outer.codec,
        payload_len: outer.payload.len(),
        columns,
    })
}

fn parse_jsonl_object(line: &str) -> Result<Map<String, Value>> {
    let value: Value =
        serde_json::from_str(line).map_err(|_| CompactError::InvalidInput("invalid jsonl"))?;
    let object = value
        .as_object()
        .ok_or(CompactError::InvalidInput("jsonl row must be an object"))?;

    Ok(object.clone())
}

fn write_column_block(
    out: &mut Vec<u8>,
    column: &ColumnSchema,
    row_count: usize,
    payload: &[u8],
) -> Result<()> {
    let name = column.name.as_bytes();
    let name_len = u16::try_from(name.len())
        .map_err(|_| CompactError::InvalidInput("column name too long"))?;
    let row_count = u64::try_from(row_count)
        .map_err(|_| CompactError::InvalidInput("jsonl row count too large"))?;
    let payload_len = u64::try_from(payload.len())
        .map_err(|_| CompactError::InvalidInput("column payload too large"))?;

    out.extend_from_slice(&name_len.to_le_bytes());
    out.extend_from_slice(name);
    out.push(codec_to_id(column.codec)?);
    out.extend_from_slice(&row_count.to_le_bytes());
    out.extend_from_slice(&payload_len.to_le_bytes());
    out.extend_from_slice(payload);

    Ok(())
}

fn decode_column_blocks(
    data: &[u8],
    schema_columns: &[ColumnSchema],
) -> Result<Vec<DecodedColumn>> {
    let mut cursor = 0usize;

    read_exact(
        data,
        &mut cursor,
        COLUMN_BLOCK_MAGIC.len(),
        "column block header is truncated",
    )
    .and_then(|magic| {
        if magic != COLUMN_BLOCK_MAGIC {
            return Err(CompactError::InvalidInput("invalid column block magic"));
        }

        Ok(())
    })?;

    let column_count = read_u32(data, &mut cursor, "column block count is truncated")? as usize;
    if column_count != schema_columns.len() {
        return Err(CompactError::InvalidInput(
            "column block count does not match schema",
        ));
    }

    let mut decoded = Vec::with_capacity(column_count);
    let mut expected_row_count = None;

    for expected_column in schema_columns {
        let name_len = read_u16(data, &mut cursor, "column name length is truncated")? as usize;
        let name_bytes = read_exact(data, &mut cursor, name_len, "column name is truncated")?;
        let name = std::str::from_utf8(name_bytes)
            .map_err(|_| CompactError::InvalidInput("column name must be utf-8"))?;

        if name != expected_column.name {
            return Err(CompactError::InvalidInput(
                "column block name does not match schema",
            ));
        }

        let codec_id = read_u8(data, &mut cursor, "column codec is truncated")?;
        if codec_id != codec_to_id(expected_column.codec)? {
            return Err(CompactError::InvalidInput(
                "column codec does not match schema",
            ));
        }

        let row_count = usize::try_from(read_u64(
            data,
            &mut cursor,
            "column row count is truncated",
        )?)
        .map_err(|_| CompactError::InvalidInput("column row count is too large"))?;
        let payload_len = usize::try_from(read_u64(
            data,
            &mut cursor,
            "column payload length is truncated",
        )?)
        .map_err(|_| CompactError::InvalidInput("column payload length is too large"))?;
        let payload = read_exact(
            data,
            &mut cursor,
            payload_len,
            "column payload is truncated",
        )?;
        let values = DecodedValues::decode(expected_column, payload)?;

        if values.len() != row_count {
            return Err(CompactError::InvalidInput(
                "decoded column row count does not match metadata",
            ));
        }

        if expected_row_count.is_some_and(|expected| expected != row_count) {
            return Err(CompactError::InvalidInput(
                "decoded columns have different row counts",
            ));
        }
        expected_row_count = Some(row_count);

        decoded.push(DecodedColumn {
            name: name.to_owned(),
            values,
        });
    }

    if cursor != data.len() {
        return Err(CompactError::InvalidInput(
            "column block has trailing bytes",
        ));
    }

    Ok(decoded)
}

fn inspect_column_blocks(data: &[u8]) -> Result<Vec<ColumnInspect>> {
    let mut cursor = 0usize;

    read_exact(
        data,
        &mut cursor,
        COLUMN_BLOCK_MAGIC.len(),
        "column block header is truncated",
    )
    .and_then(|magic| {
        if magic != COLUMN_BLOCK_MAGIC {
            return Err(CompactError::InvalidInput("invalid column block magic"));
        }

        Ok(())
    })?;

    let column_count = read_u32(data, &mut cursor, "column block count is truncated")? as usize;
    let mut columns = Vec::with_capacity(column_count);

    for _ in 0..column_count {
        let name_len = read_u16(data, &mut cursor, "column name length is truncated")? as usize;
        let name_bytes = read_exact(data, &mut cursor, name_len, "column name is truncated")?;
        let name = std::str::from_utf8(name_bytes)
            .map_err(|_| CompactError::InvalidInput("column name must be utf-8"))?;
        let codec = id_to_schema_codec(read_u8(data, &mut cursor, "column codec is truncated")?)?;
        let row_count = usize::try_from(read_u64(
            data,
            &mut cursor,
            "column row count is truncated",
        )?)
        .map_err(|_| CompactError::InvalidInput("column row count is too large"))?;
        let payload_len = usize::try_from(read_u64(
            data,
            &mut cursor,
            "column payload length is truncated",
        )?)
        .map_err(|_| CompactError::InvalidInput("column payload length is too large"))?;

        read_exact(
            data,
            &mut cursor,
            payload_len,
            "column payload is truncated",
        )?;

        columns.push(ColumnInspect {
            name: name.to_owned(),
            codec,
            row_count,
            payload_len,
        });
    }

    if cursor != data.len() {
        return Err(CompactError::InvalidInput(
            "column block has trailing bytes",
        ));
    }

    Ok(columns)
}

fn codec_to_id(codec: crate::schema::SchemaCodec) -> Result<u8> {
    match codec {
        crate::schema::SchemaCodec::Dictionary => Ok(0x05),
        crate::schema::SchemaCodec::DeltaVarintU64 => Ok(0x02),
        crate::schema::SchemaCodec::Rle => Ok(0x01),
        _ => Err(CompactError::Unsupported("v0.2 column codec")),
    }
}

fn id_to_schema_codec(id: u8) -> Result<SchemaCodec> {
    match id {
        0x01 => Ok(SchemaCodec::Rle),
        0x02 => Ok(SchemaCodec::DeltaVarintU64),
        0x05 => Ok(SchemaCodec::Dictionary),
        _ => Err(CompactError::Unsupported("column codec id")),
    }
}

fn read_exact<'a>(
    data: &'a [u8],
    cursor: &mut usize,
    len: usize,
    err: &'static str,
) -> Result<&'a [u8]> {
    let end = cursor
        .checked_add(len)
        .ok_or(CompactError::InvalidInput("column block length overflow"))?;

    if end > data.len() {
        return Err(CompactError::InvalidInput(err));
    }

    let slice = &data[*cursor..end];
    *cursor = end;

    Ok(slice)
}

fn read_u8(data: &[u8], cursor: &mut usize, err: &'static str) -> Result<u8> {
    Ok(read_exact(data, cursor, 1, err)?[0])
}

fn read_u16(data: &[u8], cursor: &mut usize, err: &'static str) -> Result<u16> {
    let bytes = read_exact(data, cursor, NAME_LEN_LEN, err)?;

    Ok(u16::from_le_bytes(
        bytes
            .try_into()
            .expect("read_exact returned exactly two bytes"),
    ))
}

fn read_u32(data: &[u8], cursor: &mut usize, err: &'static str) -> Result<u32> {
    let bytes = read_exact(data, cursor, COLUMN_COUNT_LEN, err)?;

    Ok(u32::from_le_bytes(
        bytes
            .try_into()
            .expect("read_exact returned exactly four bytes"),
    ))
}

fn read_u64(data: &[u8], cursor: &mut usize, err: &'static str) -> Result<u64> {
    let bytes = read_exact(data, cursor, ROW_COUNT_LEN, err)?;

    Ok(u64::from_le_bytes(
        bytes
            .try_into()
            .expect("read_exact returned exactly eight bytes"),
    ))
}

struct DecodedColumn {
    name: String,
    values: DecodedValues,
}

enum ColumnValues {
    U64(Vec<u64>),
    String(Vec<String>),
}

impl ColumnValues {
    fn new(column: &ColumnSchema) -> Result<Self> {
        match (column.value_type, column.codec) {
            (SchemaValueType::U64, SchemaCodec::DeltaVarintU64) => Ok(Self::U64(Vec::new())),
            (SchemaValueType::String, SchemaCodec::Dictionary | SchemaCodec::Rle) => {
                Ok(Self::String(Vec::new()))
            }
            _ => Err(CompactError::Unsupported("schema column codec")),
        }
    }

    fn push_json(&mut self, value: &Value) -> Result<()> {
        match self {
            Self::U64(values) => {
                let value = value
                    .as_u64()
                    .ok_or(CompactError::InvalidInput("jsonl column must be u64"))?;
                values.push(value);
            }
            Self::String(values) => {
                let value = value
                    .as_str()
                    .ok_or(CompactError::InvalidInput("jsonl column must be string"))?;
                values.push(value.to_owned());
            }
        }

        Ok(())
    }

    fn encode(&self, column: &ColumnSchema) -> Result<Vec<u8>> {
        match self {
            Self::U64(values) => codecs::encode_u64(&column.encode_config(), values),
            Self::String(values) => match column.codec {
                SchemaCodec::Dictionary => encode_dictionary_values(values),
                SchemaCodec::Rle => encode_string_values(values),
                _ => Err(CompactError::Unsupported("schema column codec")),
            },
        }
    }
}

enum DecodedValues {
    U64(Vec<u64>),
    String(Vec<String>),
}

impl DecodedValues {
    fn decode(column: &ColumnSchema, payload: &[u8]) -> Result<Self> {
        match (column.value_type, column.codec) {
            (SchemaValueType::U64, SchemaCodec::DeltaVarintU64) => {
                codecs::decode_u64(&column.encode_config(), payload).map(Self::U64)
            }
            (SchemaValueType::String, SchemaCodec::Rle) => {
                decode_string_values(payload).map(Self::String)
            }
            (SchemaValueType::String, SchemaCodec::Dictionary) => {
                decode_dictionary_values(payload).map(Self::String)
            }
            _ => Err(CompactError::Unsupported("schema column codec")),
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::U64(values) => values.len(),
            Self::String(values) => values.len(),
        }
    }

    fn value_at(&self, index: usize) -> Value {
        match self {
            Self::U64(values) => Value::from(values[index]),
            Self::String(values) => Value::from(values[index].clone()),
        }
    }
}

fn encode_string_values(values: &[String]) -> Result<Vec<u8>> {
    let mut raw = Vec::new();

    for value in values {
        let bytes = value.as_bytes();
        let len = u64::try_from(bytes.len())
            .map_err(|_| CompactError::InvalidInput("string value too large"))?;
        raw.extend_from_slice(&varint::encode_u64(&[len]));
        raw.extend_from_slice(bytes);
    }

    Ok(rle::encode_rle(&raw))
}

fn encode_dictionary_values(values: &[String]) -> Result<Vec<u8>> {
    let mut dictionary = Vec::<String>::new();
    let mut ids = Vec::<u64>::with_capacity(values.len());
    let mut index_by_value = HashMap::<&str, u64>::new();

    for value in values {
        let id = if let Some(&id) = index_by_value.get(value.as_str()) {
            id
        } else {
            let id = u64::try_from(dictionary.len())
                .map_err(|_| CompactError::InvalidInput("dictionary has too many values"))?;
            dictionary.push(value.clone());
            index_by_value.insert(value.as_str(), id);
            id
        };

        ids.push(id);
    }

    let mut raw = Vec::new();
    raw.extend_from_slice(&varint::encode_u64(&[dictionary.len() as u64]));

    for value in dictionary {
        let bytes = value.as_bytes();
        let len = u64::try_from(bytes.len())
            .map_err(|_| CompactError::InvalidInput("string value too large"))?;
        raw.extend_from_slice(&varint::encode_u64(&[len]));
        raw.extend_from_slice(bytes);
    }

    raw.extend_from_slice(&varint::encode_u64(&ids));

    Ok(raw)
}

fn decode_string_values(payload: &[u8]) -> Result<Vec<String>> {
    let raw = rle::decode_rle(payload)?;
    let mut cursor = 0usize;
    let mut values = Vec::new();

    while cursor < raw.len() {
        let len = read_varint_u64(&raw, &mut cursor)?;
        let len = usize::try_from(len)
            .map_err(|_| CompactError::InvalidInput("string length is too large"))?;
        let bytes = read_exact(&raw, &mut cursor, len, "string payload is truncated")?;
        let value = std::str::from_utf8(bytes)
            .map_err(|_| CompactError::InvalidInput("string value must be utf-8"))?;

        values.push(value.to_owned());
    }

    Ok(values)
}

fn decode_dictionary_values(payload: &[u8]) -> Result<Vec<String>> {
    let mut cursor = 0usize;
    let dictionary_len = usize::try_from(read_varint_u64(payload, &mut cursor)?)
        .map_err(|_| CompactError::InvalidInput("dictionary length is too large"))?;
    let mut dictionary = Vec::with_capacity(dictionary_len);

    for _ in 0..dictionary_len {
        let len = usize::try_from(read_varint_u64(payload, &mut cursor)?)
            .map_err(|_| CompactError::InvalidInput("string length is too large"))?;
        let bytes = read_exact(payload, &mut cursor, len, "dictionary string is truncated")?;
        let value = std::str::from_utf8(bytes)
            .map_err(|_| CompactError::InvalidInput("string value must be utf-8"))?;
        dictionary.push(value.to_owned());
    }

    let ids = varint::decode_u64(&payload[cursor..])?;
    let mut values = Vec::with_capacity(ids.len());

    for id in ids {
        let index = usize::try_from(id)
            .map_err(|_| CompactError::InvalidInput("dictionary id is too large"))?;
        let value = dictionary
            .get(index)
            .ok_or(CompactError::InvalidInput("dictionary id out of range"))?;
        values.push(value.clone());
    }

    Ok(values)
}

fn read_varint_u64(data: &[u8], cursor: &mut usize) -> Result<u64> {
    let start = *cursor;

    while *cursor < data.len() {
        let byte = data[*cursor];
        *cursor += 1;

        if byte & 0x80 == 0 {
            let decoded = varint::decode_u64(&data[start..*cursor])?;

            return decoded
                .first()
                .copied()
                .ok_or(CompactError::InvalidInput("truncated varint"));
        }
    }

    Err(CompactError::InvalidInput("truncated varint"))
}

fn render_json_object(columns: &[DecodedColumn], row_index: usize) -> Result<String> {
    let mut out = String::from("{");

    for (index, column) in columns.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }

        out.push_str(
            &serde_json::to_string(&column.name)
                .map_err(|_| CompactError::InvalidInput("json key cannot be serialized"))?,
        );
        out.push(':');
        out.push_str(&column.values.value_at(row_index).to_string());
    }

    out.push('}');

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{decode_jsonl, encode_jsonl, encode_jsonl_row_group};
    use crate::CompactError;
    use crate::schema::Schema;

    fn single_column_schema() -> Schema {
        Schema::from_yaml(
            r#"
columns:
  - name: ts
    type: u64
    codec: delta_varint_u64
"#,
        )
        .unwrap()
    }

    fn two_column_schema() -> Schema {
        Schema::from_yaml(
            r#"
columns:
  - name: ts
    type: u64
    codec: delta_varint_u64
  - name: user_id
    type: u64
    codec: delta_varint_u64
"#,
        )
        .unwrap()
    }

    fn mixed_schema() -> Schema {
        Schema::from_yaml(
            r#"
columns:
  - name: ts
    type: u64
    codec: delta_varint_u64
  - name: level
    type: string
    codec: rle
  - name: message
    type: string
    codec: rle
"#,
        )
        .unwrap()
    }

    #[test]
    fn jsonl_single_u64_column_roundtrip_is_byte_stable() {
        let input = "{\"ts\":100}\n{\"ts\":101}\n{\"ts\":130}\n";
        let encoded = encode_jsonl(input, &single_column_schema()).unwrap();
        let decoded = decode_jsonl(&encoded, &single_column_schema()).unwrap();

        assert_eq!(decoded, input);
    }

    #[test]
    fn jsonl_row_group_exposes_streaming_block_inputs() {
        let input = "{\"ts\":100}\n{\"ts\":101}\n";
        let row_group = encode_jsonl_row_group(input, &single_column_schema()).unwrap();

        assert_eq!(row_group.row_count, 2);
        assert_eq!(row_group.raw_bytes, input.len());
        assert!(!row_group.payload.is_empty());
    }

    #[test]
    fn jsonl_empty_row_group_has_column_metadata_without_rows() {
        let row_group = encode_jsonl_row_group("", &single_column_schema()).unwrap();

        assert_eq!(row_group.row_count, 0);
        assert_eq!(row_group.raw_bytes, 0);
        assert!(!row_group.payload.is_empty());
    }

    #[test]
    fn jsonl_two_u64_columns_roundtrip_is_byte_stable() {
        let input =
            "{\"ts\":100,\"user_id\":7}\n{\"ts\":101,\"user_id\":7}\n{\"ts\":130,\"user_id\":8}\n";
        let encoded = encode_jsonl(input, &two_column_schema()).unwrap();
        let decoded = decode_jsonl(&encoded, &two_column_schema()).unwrap();

        assert_eq!(decoded, input);
    }

    #[test]
    fn jsonl_mixed_u64_string_columns_roundtrip_is_byte_stable() {
        let input = "{\"ts\":100,\"level\":\"INFO\",\"message\":\"\"}\n{\"ts\":101,\"level\":\"INFO\",\"message\":\"hello\"}\n{\"ts\":130,\"level\":\"WARN\",\"message\":\"xin chao\"}\n";
        let encoded = encode_jsonl(input, &mixed_schema()).unwrap();
        let decoded = decode_jsonl(&encoded, &mixed_schema()).unwrap();

        assert_eq!(decoded, input);
    }

    #[test]
    fn jsonl_rejects_missing_column() {
        let err = encode_jsonl("{\"id\":100}\n", &single_column_schema()).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("jsonl row missing schema column")
        ));
    }

    #[test]
    fn jsonl_rejects_negative_u64_column() {
        let err = encode_jsonl("{\"ts\":-1}\n", &single_column_schema()).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("jsonl column must be u64")
        ));
    }

    #[test]
    fn jsonl_rejects_non_string_column() {
        let err = encode_jsonl(
            "{\"ts\":100,\"level\":1,\"message\":\"hello\"}\n",
            &mixed_schema(),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("jsonl column must be string")
        ));
    }

    #[test]
    fn jsonl_rejects_invalid_json() {
        let err = encode_jsonl("{bad json}\n", &single_column_schema()).unwrap_err();

        assert!(matches!(err, CompactError::InvalidInput("invalid jsonl")));
    }

    #[test]
    fn jsonl_decode_rejects_schema_mismatch() {
        let input = "{\"ts\":100}\n";
        let encoded = encode_jsonl(input, &single_column_schema()).unwrap();
        let err = decode_jsonl(&encoded, &two_column_schema()).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("column block count does not match schema")
        ));
    }

    #[test]
    fn string_payload_rejects_truncated_string_bytes() {
        let encoded = super::rle::encode_rle(&[3, b'a']);
        let err = super::decode_string_values(&encoded).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("string payload is truncated")
        ));
    }
}
