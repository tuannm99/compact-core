//! End-to-end CMP3 JSONL support available after Phase 3.
//!
//! This module writes one independently checksummed row group containing
//! boolean bitmap columns. Numeric and string columns remain unsupported until
//! their CMP3 codecs land. The format is deliberately one-row-group for now;
//! later streaming work can reuse the row-group body without changing boolean
//! column payloads.

use serde_json::{Map, Value};

use crate::codecs::v3::boolean::{decode_boolean_column, encode_boolean_column};
use crate::format::v3::{
    ColumnChunkMetadata, decode_column_metadata, decode_header, encode_column_metadata,
    encode_empty_header,
};
use crate::primitives::crc32;
use crate::schema::{ColumnSchema, Schema, SchemaCodec, SchemaValueType};
use crate::{CompactError, Result};

const ROW_GROUP_MAGIC: [u8; 4] = *b"RGB3";
const CHECKSUM_LEN: usize = 4;

/// Encode JSONL into a single-row-group CMP3 file.
///
/// Phase 3 supports explicit `bool` columns using the `bitmap` codec. Nullable
/// fields may be missing or JSON null; both decode as an explicit null field.
pub fn encode_jsonl(input: &str, schema: &Schema) -> Result<Vec<u8>> {
    let columns = validate_phase3_schema(schema)?;
    let rows = parse_rows(input)?;
    let row_count = u64::try_from(rows.len())
        .map_err(|_| CompactError::InvalidInput("cmp3 row count is too large"))?;
    let raw_size = u64::try_from(input.len())
        .map_err(|_| CompactError::InvalidInput("cmp3 raw jsonl size is too large"))?;
    let column_count = u32::try_from(columns.len())
        .map_err(|_| CompactError::InvalidInput("cmp3 schema has too many columns"))?;

    let mut row_group = Vec::new();
    row_group.extend_from_slice(&ROW_GROUP_MAGIC);
    row_group.extend_from_slice(&0u64.to_le_bytes()); // block index
    row_group.extend_from_slice(&0u64.to_le_bytes()); // first row index
    row_group.extend_from_slice(&row_count.to_le_bytes());
    row_group.extend_from_slice(&raw_size.to_le_bytes());
    row_group.extend_from_slice(&column_count.to_le_bytes());

    for column in columns {
        let chunk = encode_boolean_column(column, &rows)?;
        let metadata = encode_column_metadata(&chunk.metadata)?;
        let metadata_len = u32::try_from(metadata.len())
            .map_err(|_| CompactError::InvalidInput("cmp3 column metadata is too large"))?;

        row_group.extend_from_slice(&metadata_len.to_le_bytes());
        row_group.extend_from_slice(&metadata);
        row_group.extend_from_slice(&chunk.payload);
    }

    let checksum = crc32::checksum(&row_group);
    row_group.extend_from_slice(&checksum.to_le_bytes());

    let mut file = encode_empty_header();
    file.extend_from_slice(&row_group);
    Ok(file)
}

/// Decode a single-row-group CMP3 boolean file into canonical JSONL.
pub fn decode_jsonl(data: &[u8], schema: &Schema) -> Result<String> {
    let columns = validate_phase3_schema(schema)?;
    let header = decode_header(data)?;
    if !header.payload.is_empty() {
        return Err(CompactError::Unsupported("cmp3 header payload"));
    }

    let row_group = data
        .get(header.body_offset..)
        .ok_or(CompactError::InvalidInput("cmp3 row group is truncated"))?;
    if row_group.len() < CHECKSUM_LEN {
        return Err(CompactError::InvalidInput("cmp3 row group is truncated"));
    }
    let checksum_offset = row_group.len() - CHECKSUM_LEN;
    let stored_checksum = u32::from_le_bytes(
        row_group[checksum_offset..]
            .try_into()
            .expect("checksum suffix contains four bytes"),
    );
    if !crc32::verify(&row_group[..checksum_offset], stored_checksum) {
        return Err(CompactError::InvalidInput(
            "cmp3 row group checksum mismatch",
        ));
    }

    let body = &row_group[..checksum_offset];
    let mut cursor = 0usize;
    if read_exact(body, &mut cursor, 4, "cmp3 row group magic is truncated")? != ROW_GROUP_MAGIC {
        return Err(CompactError::InvalidInput("invalid cmp3 row group magic"));
    }
    if read_u64(body, &mut cursor, "cmp3 block index is truncated")? != 0 {
        return Err(CompactError::InvalidInput(
            "cmp3 single row group must have block index zero",
        ));
    }
    if read_u64(body, &mut cursor, "cmp3 first row index is truncated")? != 0 {
        return Err(CompactError::InvalidInput(
            "cmp3 single row group must start at row zero",
        ));
    }
    let row_count = usize::try_from(read_u64(body, &mut cursor, "cmp3 row count is truncated")?)
        .map_err(|_| CompactError::InvalidInput("cmp3 row count is too large"))?;
    let _raw_size = read_u64(body, &mut cursor, "cmp3 raw jsonl size is truncated")?;
    let column_count = usize::try_from(read_u32(
        body,
        &mut cursor,
        "cmp3 column count is truncated",
    )?)
    .map_err(|_| CompactError::InvalidInput("cmp3 column count is too large"))?;
    if column_count != columns.len() {
        return Err(CompactError::InvalidInput(
            "cmp3 column count does not match schema",
        ));
    }

    let mut decoded_columns = Vec::with_capacity(column_count);
    for column in columns {
        let metadata_len = usize::try_from(read_u32(
            body,
            &mut cursor,
            "cmp3 column metadata length is truncated",
        )?)
        .map_err(|_| CompactError::InvalidInput("cmp3 column metadata is too large"))?;
        let metadata_bytes = read_exact(
            body,
            &mut cursor,
            metadata_len,
            "cmp3 column metadata is truncated",
        )?;
        let (metadata, consumed) = decode_column_metadata(metadata_bytes)?;
        if consumed != metadata_bytes.len() {
            return Err(CompactError::InvalidInput(
                "cmp3 column metadata has trailing bytes",
            ));
        }
        validate_metadata_against_schema(&metadata, column, row_count)?;

        let payload_len = usize::try_from(metadata.compressed_size)
            .map_err(|_| CompactError::InvalidInput("cmp3 column payload is too large"))?;
        let payload = read_exact(
            body,
            &mut cursor,
            payload_len,
            "cmp3 column payload is truncated",
        )?;
        decoded_columns.push(decode_boolean_column(&metadata, payload)?);
    }

    if cursor != body.len() {
        return Err(CompactError::InvalidInput(
            "cmp3 row group has trailing bytes",
        ));
    }

    render_rows(columns, &decoded_columns, row_count)
}

fn validate_phase3_schema(schema: &Schema) -> Result<&[ColumnSchema]> {
    let columns = schema.supported_columns_v3()?;

    if columns.iter().any(|column| {
        column.value_type != SchemaValueType::Bool || column.codec != SchemaCodec::Bitmap
    }) {
        return Err(CompactError::Unsupported(
            "phase 3 cmp3 jsonl requires bool bitmap columns",
        ));
    }

    Ok(columns)
}

fn parse_rows(input: &str) -> Result<Vec<Map<String, Value>>> {
    input
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let value: Value = serde_json::from_str(line)
                .map_err(|_| CompactError::InvalidInput("invalid jsonl"))?;
            value
                .as_object()
                .cloned()
                .ok_or(CompactError::InvalidInput("jsonl row must be an object"))
        })
        .collect()
}

fn validate_metadata_against_schema(
    metadata: &ColumnChunkMetadata,
    column: &ColumnSchema,
    row_count: usize,
) -> Result<()> {
    if metadata.name != column.name
        || metadata.value_type != column.value_type
        || metadata.nullable != column.nullable
        || metadata.codec != column.codec
    {
        return Err(CompactError::InvalidInput(
            "cmp3 column metadata does not match schema",
        ));
    }
    if metadata.value_count
        != u64::try_from(row_count)
            .map_err(|_| CompactError::InvalidInput("cmp3 row count is too large"))?
    {
        return Err(CompactError::InvalidInput(
            "cmp3 column value count does not match row group",
        ));
    }

    Ok(())
}

fn render_rows(
    columns: &[ColumnSchema],
    decoded_columns: &[Vec<Option<bool>>],
    row_count: usize,
) -> Result<String> {
    let mut out = String::new();

    for row_index in 0..row_count {
        let mut row = Map::with_capacity(columns.len());
        for (column, values) in columns.iter().zip(decoded_columns) {
            let value = values
                .get(row_index)
                .ok_or(CompactError::InvalidInput(
                    "cmp3 decoded column is shorter than row count",
                ))?
                .map_or(Value::Null, Value::Bool);
            row.insert(column.name.clone(), value);
        }
        out.push_str(
            &serde_json::to_string(&row)
                .map_err(|_| CompactError::InvalidInput("json row cannot be serialized"))?,
        );
        out.push('\n');
    }

    Ok(out)
}

fn read_exact<'a>(
    data: &'a [u8],
    cursor: &mut usize,
    len: usize,
    err: &'static str,
) -> Result<&'a [u8]> {
    let end = cursor
        .checked_add(len)
        .ok_or(CompactError::InvalidInput("cmp3 row group length overflow"))?;
    if end > data.len() {
        return Err(CompactError::InvalidInput(err));
    }

    let bytes = &data[*cursor..end];
    *cursor = end;
    Ok(bytes)
}

fn read_u32(data: &[u8], cursor: &mut usize, err: &'static str) -> Result<u32> {
    Ok(u32::from_le_bytes(
        read_exact(data, cursor, 4, err)?
            .try_into()
            .expect("read_exact returned four bytes"),
    ))
}

fn read_u64(data: &[u8], cursor: &mut usize, err: &'static str) -> Result<u64> {
    Ok(u64::from_le_bytes(
        read_exact(data, cursor, 8, err)?
            .try_into()
            .expect("read_exact returned eight bytes"),
    ))
}

#[cfg(test)]
mod tests {
    use super::{decode_jsonl, encode_jsonl};
    use crate::CompactError;
    use crate::primitives::crc32;
    use crate::schema::Schema;

    fn schema() -> Schema {
        Schema::from_yaml(
            r#"
columns:
  - name: active
    type: bool
    codec: bitmap
  - name: sampled
    type: bool
    codec: bitmap
    nullable: true
"#,
        )
        .unwrap()
    }

    fn replace_checksum(encoded: &mut [u8]) {
        let checksum_offset = encoded.len() - 4;
        let checksum = crc32::checksum(&encoded[10..checksum_offset]);
        encoded[checksum_offset..].copy_from_slice(&checksum.to_le_bytes());
    }

    #[test]
    fn cmp3_boolean_jsonl_roundtrips_required_nullable_and_missing_values() {
        let input = "{\"active\":true,\"sampled\":false}\n{\"active\":false,\"sampled\":null}\n{\"active\":true}\n";
        let encoded = encode_jsonl(input, &schema()).unwrap();
        let decoded = decode_jsonl(&encoded, &schema()).unwrap();

        assert_eq!(
            decoded,
            "{\"active\":true,\"sampled\":false}\n{\"active\":false,\"sampled\":null}\n{\"active\":true,\"sampled\":null}\n"
        );
    }

    #[test]
    fn cmp3_empty_jsonl_roundtrips() {
        let encoded = encode_jsonl("", &schema()).unwrap();
        let decoded = decode_jsonl(&encoded, &schema()).unwrap();

        assert!(decoded.is_empty());
    }

    #[test]
    fn cmp3_boolean_jsonl_encoding_is_deterministic() {
        let input = "{\"active\":true}\n{\"active\":false,\"sampled\":true}\n";

        assert_eq!(
            encode_jsonl(input, &schema()).unwrap(),
            encode_jsonl(input, &schema()).unwrap()
        );
    }

    #[test]
    fn cmp3_boolean_jsonl_rejects_corrupted_row_group() {
        let mut encoded = encode_jsonl("{\"active\":true}\n", &schema()).unwrap();
        let payload_offset = 10 + 4 + 8 + 8 + 8 + 8 + 4;
        encoded[payload_offset] ^= 1;
        let err = decode_jsonl(&encoded, &schema()).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("cmp3 row group checksum mismatch")
        ));
    }

    #[test]
    fn cmp3_boolean_jsonl_rejects_truncated_file() {
        let mut encoded = encode_jsonl("{\"active\":true}\n", &schema()).unwrap();
        encoded.pop();
        let err = decode_jsonl(&encoded, &schema()).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput(
                "cmp3 row group checksum mismatch" | "cmp3 row group is truncated"
            )
        ));
    }

    #[test]
    fn cmp3_boolean_jsonl_rejects_metadata_length_beyond_row_group() {
        let mut encoded = encode_jsonl("{\"active\":true}\n", &schema()).unwrap();
        let first_metadata_len_offset = 10 + 4 + 8 + 8 + 8 + 8 + 4;
        encoded[first_metadata_len_offset..first_metadata_len_offset + 4]
            .copy_from_slice(&u32::MAX.to_le_bytes());
        replace_checksum(&mut encoded);
        let err = decode_jsonl(&encoded, &schema()).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("cmp3 column metadata is truncated")
        ));
    }

    #[test]
    fn cmp3_boolean_jsonl_rejects_authenticated_trailing_bytes() {
        let mut encoded = encode_jsonl("{\"active\":true}\n", &schema()).unwrap();
        let checksum_offset = encoded.len() - 4;
        encoded.insert(checksum_offset, 0);
        replace_checksum(&mut encoded);
        let err = decode_jsonl(&encoded, &schema()).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("cmp3 row group has trailing bytes")
        ));
    }

    #[test]
    fn cmp3_boolean_jsonl_rejects_schema_mismatch() {
        let encoded = encode_jsonl("{\"active\":true}\n", &schema()).unwrap();
        let other = Schema::from_yaml(
            r#"
columns:
  - name: enabled
    type: bool
    codec: bitmap
  - name: sampled
    type: bool
    codec: bitmap
    nullable: true
"#,
        )
        .unwrap();
        let err = decode_jsonl(&encoded, &other).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("cmp3 column metadata does not match schema")
        ));
    }

    #[test]
    fn cmp3_boolean_jsonl_rejects_unimplemented_column_types() {
        let numeric = Schema::from_yaml(
            r#"
columns:
  - name: ts
    type: u64
    codec: bitpack
"#,
        )
        .unwrap();
        let err = encode_jsonl("{\"ts\":1}\n", &numeric).unwrap_err();

        assert!(matches!(
            err,
            CompactError::Unsupported("phase 3 cmp3 jsonl requires bool bitmap columns")
        ));
    }
}
