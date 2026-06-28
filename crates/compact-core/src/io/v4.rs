//! End-to-end CMP4 JSONL support for queryable row groups.
//!
//! CMP4 reuses the CMP3 column codecs, but writes multiple row groups and an
//! EOF footer index. The footer lets readers inspect metadata first, then decode
//! only selected row groups and columns for projection or predicate scans.

use std::collections::HashSet;

use serde_json::{Map, Value};

use crate::format::v3::{ColumnChunkMetadata, decode_column_metadata, encode_column_metadata};
use crate::format::v4::{
    ColumnIndexEntry, FooterIndex, RowGroupIndexEntry, append_footer, decode_footer,
    decode_footer_trailer, decode_header, encode_empty_header,
};
use crate::io::v3::{
    DecodedColumn, decode_column, encode_column, parse_rows, validate_implemented_schema,
    validate_metadata_against_schema,
};
use crate::limits::MAX_COLLECTION_ENTRIES;
use crate::primitives::crc32;
use crate::schema::{ColumnSchema, Schema, SchemaValueType};
use crate::statistics::ColumnStatistics;
use crate::{CompactError, Result};

const ROW_GROUP_MAGIC: [u8; 4] = *b"RGB4";
const CHECKSUM_LEN: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodeOptions {
    pub row_group_rows: usize,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self {
            row_group_rows: 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanResult {
    pub jsonl: String,
    pub row_groups_scanned: usize,
    pub row_groups_pruned: usize,
}

/// Schema-independent CMP4 prefix recovered from contiguous valid row groups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRecovery {
    /// Byte offset immediately after the last valid row group.
    pub valid_body_len: u64,
    /// Reconstructed footer metadata for the valid prefix.
    pub footer: FooterIndex,
    /// Whether bytes after `valid_body_len` were not a valid current footer.
    pub discarded_tail: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Predicate {
    U64 { column: String, op: U64PredicateOp },
    IsNull { column: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum U64PredicateOp {
    Eq(u64),
    Lt(u64),
    Le(u64),
    Gt(u64),
    Ge(u64),
}

/// Encode JSONL into a multi-row-group CMP4 file.
pub fn encode_jsonl(input: &str, schema: &Schema, options: EncodeOptions) -> Result<Vec<u8>> {
    if options.row_group_rows == 0 {
        return Err(CompactError::InvalidInput(
            "cmp4 row group row limit must be positive",
        ));
    }

    let columns = validate_implemented_schema(schema)?;
    let rows = parse_rows(input)?;
    let mut file = encode_empty_header();
    let mut row_groups = Vec::new();

    for (row_group_index, chunk) in rows.chunks(options.row_group_rows).enumerate() {
        let first_row_index = row_group_index
            .checked_mul(options.row_group_rows)
            .ok_or(CompactError::InvalidInput("cmp4 first row index overflow"))?;
        let (row_group, entry) = encode_row_group(
            row_group_index,
            first_row_index,
            chunk,
            columns,
            u64::try_from(file.len())
                .map_err(|_| CompactError::InvalidInput("cmp4 file length is too large"))?,
        )?;
        file.extend_from_slice(&row_group);
        row_groups.push(entry);
    }

    let total_row_count = u64::try_from(rows.len())
        .map_err(|_| CompactError::InvalidInput("cmp4 row count is too large"))?;
    let footer = FooterIndex {
        total_row_count,
        row_groups,
    };
    append_footer(&mut file, &footer)?;

    Ok(file)
}

/// Decode all columns from a CMP4 file.
pub fn decode_jsonl(data: &[u8], schema: &Schema) -> Result<String> {
    scan_jsonl(data, schema, &[], None).map(|result| result.jsonl)
}

/// Decode only the requested columns from a CMP4 file.
pub fn decode_jsonl_projected(data: &[u8], schema: &Schema, projection: &[&str]) -> Result<String> {
    scan_jsonl(data, schema, projection, None).map(|result| result.jsonl)
}

/// Scan CMP4 with optional projection and predicate pushdown.
pub fn scan_jsonl(
    data: &[u8],
    schema: &Schema,
    projection: &[&str],
    predicate: Option<&Predicate>,
) -> Result<ScanResult> {
    let columns = validate_implemented_schema(schema)?;
    let header = decode_header(data)?;
    if !header.payload.is_empty() {
        return Err(CompactError::Unsupported("cmp4 header payload"));
    }
    let footer = decode_footer(data)?;
    let projected_columns = select_projection(columns, projection)?;
    validate_predicate(columns, predicate)?;

    let mut jsonl = String::new();
    let mut row_groups_scanned = 0usize;
    let mut row_groups_pruned = 0usize;

    for row_group in &footer.row_groups {
        if !row_group_may_match(row_group, predicate)? {
            row_groups_pruned += 1;
            continue;
        }

        let decoded = decode_row_group_projection(
            data,
            row_group,
            columns,
            &projected_columns,
            predicate,
            true,
        )?;
        render_filtered_rows(&mut jsonl, &projected_columns, &decoded, predicate)?;
        row_groups_scanned += 1;
    }

    Ok(ScanResult {
        jsonl,
        row_groups_scanned,
        row_groups_pruned,
    })
}

/// Read only CMP4 footer metadata.
pub fn inspect_footer(data: &[u8]) -> Result<FooterIndex> {
    decode_header(data)?;
    decode_footer(data)
}

/// Validate the complete schema-independent CMP4 storage envelope.
///
/// The footer parser validates index ranges and ordering. This additional pass
/// verifies that row groups cover the body without gaps, each row-group header
/// agrees with the footer, and every row-group checksum is valid.
pub fn validate_file(data: &[u8]) -> Result<FooterIndex> {
    let header = decode_header(data)?;
    if !header.payload.is_empty() {
        return Err(CompactError::Unsupported("cmp4 header payload"));
    }
    let trailer = decode_footer_trailer(data)?;
    let footer = decode_footer(data)?;
    let mut expected_offset = u64::try_from(header.body_offset)
        .map_err(|_| CompactError::InvalidInput("cmp4 header offset is too large"))?;

    for row_group in &footer.row_groups {
        if row_group.row_group_offset != expected_offset {
            return Err(CompactError::InvalidInput(
                "cmp4 row groups must be physically contiguous",
            ));
        }
        verify_row_group(data, row_group, true)?;
        expected_offset = expected_offset
            .checked_add(row_group.row_group_len)
            .ok_or(CompactError::InvalidInput("cmp4 row group range overflow"))?;
    }

    if expected_offset != trailer.footer_offset {
        return Err(CompactError::InvalidInput(
            "cmp4 body does not end at footer",
        ));
    }

    Ok(footer)
}

/// Recover a contiguous CMP4 row-group prefix without requiring a schema.
///
/// Recovery starts after the checked file header and stops at the first
/// malformed or checksum-invalid row group. It never searches for a later
/// `RGB4` marker because bytes after a failed boundary are not trustworthy.
pub fn recover_file_prefix(data: &[u8]) -> Result<FileRecovery> {
    let header = decode_header(data)?;
    if !header.payload.is_empty() {
        return Err(CompactError::Unsupported("cmp4 header payload"));
    }

    let mut cursor = header.body_offset;
    let mut row_groups = Vec::new();
    let mut expected_first_row = 0u64;

    while data
        .get(cursor..cursor.saturating_add(ROW_GROUP_MAGIC.len()))
        .is_some_and(|magic| magic == ROW_GROUP_MAGIC)
    {
        let row_group_index = u64::try_from(row_groups.len())
            .map_err(|_| CompactError::InvalidInput("cmp4 row group count is too large"))?;
        let Ok((row_group, next_cursor)) =
            recover_row_group(data, cursor, row_group_index, expected_first_row)
        else {
            break;
        };
        expected_first_row = expected_first_row
            .checked_add(row_group.row_count)
            .ok_or(CompactError::InvalidInput("cmp4 total row count overflow"))?;
        row_groups.push(row_group);
        cursor = next_cursor;
    }

    let valid_body_len = u64::try_from(cursor)
        .map_err(|_| CompactError::InvalidInput("cmp4 recovery offset is too large"))?;
    let footer = FooterIndex {
        total_row_count: expected_first_row,
        row_groups,
    };

    Ok(FileRecovery {
        valid_body_len,
        footer,
        discarded_tail: cursor < data.len(),
    })
}

fn recover_row_group(
    data: &[u8],
    start: usize,
    expected_index: u64,
    expected_first_row: u64,
) -> Result<(RowGroupIndexEntry, usize)> {
    let mut cursor = start;
    if read_exact(
        data,
        &mut cursor,
        ROW_GROUP_MAGIC.len(),
        "cmp4 row group magic is truncated",
    )? != ROW_GROUP_MAGIC
    {
        return Err(CompactError::InvalidInput("invalid cmp4 row group magic"));
    }
    if read_u64(data, &mut cursor, "cmp4 row group index is truncated")? != expected_index {
        return Err(CompactError::InvalidInput(
            "cmp4 row group index is not sequential",
        ));
    }
    if read_u64(data, &mut cursor, "cmp4 first row index is truncated")? != expected_first_row {
        return Err(CompactError::InvalidInput(
            "cmp4 first row index is not sequential",
        ));
    }
    let row_count = read_u64(data, &mut cursor, "cmp4 row count is truncated")?;
    if row_count == 0 {
        return Err(CompactError::InvalidInput(
            "cmp4 row group must contain rows",
        ));
    }
    read_u64(data, &mut cursor, "cmp4 raw jsonl size is truncated")?;
    let column_count = usize::try_from(read_u32(
        data,
        &mut cursor,
        "cmp4 column count is truncated",
    )?)
    .map_err(|_| CompactError::InvalidInput("cmp4 column count is too large"))?;
    if column_count > MAX_COLLECTION_ENTRIES
        || column_count > data.len().saturating_sub(cursor) / size_of::<u32>()
    {
        return Err(CompactError::InvalidInput(
            "cmp4 column count exceeds row group bounds",
        ));
    }
    let mut columns = Vec::with_capacity(column_count);
    let mut names = HashSet::new();

    for _ in 0..column_count {
        let metadata_len = usize::try_from(read_u32(
            data,
            &mut cursor,
            "cmp4 column metadata length is truncated",
        )?)
        .map_err(|_| CompactError::InvalidInput("cmp4 column metadata is too large"))?;
        let metadata_offset = u64::try_from(cursor)
            .map_err(|_| CompactError::InvalidInput("cmp4 metadata offset is too large"))?;
        let metadata_bytes = read_exact(
            data,
            &mut cursor,
            metadata_len,
            "cmp4 column metadata is truncated",
        )?;
        let (metadata, consumed) = decode_column_metadata(metadata_bytes)?;
        if consumed != metadata_bytes.len() {
            return Err(CompactError::InvalidInput(
                "cmp4 column metadata has trailing bytes",
            ));
        }
        if metadata.value_count != row_count {
            return Err(CompactError::InvalidInput(
                "cmp4 column value count does not match row group",
            ));
        }
        if !names.insert(metadata.name.clone()) {
            return Err(CompactError::InvalidInput(
                "cmp4 duplicate column in row group",
            ));
        }
        crate::statistics::decode(&metadata.statistics_metadata)?;

        let payload_offset = u64::try_from(cursor)
            .map_err(|_| CompactError::InvalidInput("cmp4 payload offset is too large"))?;
        let payload_len = usize::try_from(metadata.compressed_size)
            .map_err(|_| CompactError::InvalidInput("cmp4 column payload is too large"))?;
        read_exact(
            data,
            &mut cursor,
            payload_len,
            "cmp4 column payload is truncated",
        )?;

        columns.push(ColumnIndexEntry {
            name: metadata.name,
            metadata_offset,
            metadata_len: u64::try_from(metadata_len)
                .map_err(|_| CompactError::InvalidInput("cmp4 metadata length is too large"))?,
            payload_offset,
            payload_len: metadata.compressed_size,
            value_count: metadata.value_count,
            null_count: metadata.null_count,
            statistics_metadata: metadata.statistics_metadata,
        });
    }

    let checksum_offset = cursor;
    let stored_checksum = read_u32(data, &mut cursor, "cmp4 row group checksum is truncated")?;
    if !crc32::verify(&data[start..checksum_offset], stored_checksum) {
        return Err(CompactError::InvalidInput(
            "cmp4 row group checksum mismatch",
        ));
    }

    let row_group_offset = u64::try_from(start)
        .map_err(|_| CompactError::InvalidInput("cmp4 row group offset is too large"))?;
    let row_group_len = u64::try_from(cursor - start)
        .map_err(|_| CompactError::InvalidInput("cmp4 row group length is too large"))?;
    Ok((
        RowGroupIndexEntry {
            row_group_index: expected_index,
            first_row_index: expected_first_row,
            row_count,
            row_group_offset,
            row_group_len,
            columns,
        },
        cursor,
    ))
}

fn encode_row_group(
    row_group_index: usize,
    first_row_index: usize,
    rows: &[Map<String, Value>],
    columns: &[ColumnSchema],
    row_group_offset: u64,
) -> Result<(Vec<u8>, RowGroupIndexEntry)> {
    let row_count = u64::try_from(rows.len())
        .map_err(|_| CompactError::InvalidInput("cmp4 row count is too large"))?;
    let column_count = u32::try_from(columns.len())
        .map_err(|_| CompactError::InvalidInput("cmp4 schema has too many columns"))?;
    let mut row_group = Vec::new();
    let mut column_entries = Vec::with_capacity(columns.len());

    row_group.extend_from_slice(&ROW_GROUP_MAGIC);
    row_group.extend_from_slice(
        &u64::try_from(row_group_index)
            .map_err(|_| CompactError::InvalidInput("cmp4 row group index is too large"))?
            .to_le_bytes(),
    );
    row_group.extend_from_slice(
        &u64::try_from(first_row_index)
            .map_err(|_| CompactError::InvalidInput("cmp4 first row index is too large"))?
            .to_le_bytes(),
    );
    row_group.extend_from_slice(&row_count.to_le_bytes());
    row_group.extend_from_slice(&0u64.to_le_bytes()); // raw JSONL bytes are not required for query planning.
    row_group.extend_from_slice(&column_count.to_le_bytes());

    for column in columns {
        let chunk = encode_column(column, rows)?;
        let metadata = encode_column_metadata(&chunk.metadata)?;
        let metadata_len = u32::try_from(metadata.len())
            .map_err(|_| CompactError::InvalidInput("cmp4 column metadata is too large"))?;
        let metadata_len_offset = row_group.len();
        row_group.extend_from_slice(&metadata_len.to_le_bytes());
        let metadata_offset = absolute_offset(row_group_offset, row_group.len())?;
        row_group.extend_from_slice(&metadata);
        let payload_offset = absolute_offset(row_group_offset, row_group.len())?;
        row_group.extend_from_slice(&chunk.payload);

        column_entries.push(ColumnIndexEntry {
            name: column.name.clone(),
            metadata_offset,
            metadata_len: u64::from(metadata_len),
            payload_offset,
            payload_len: u64::try_from(chunk.payload.len())
                .map_err(|_| CompactError::InvalidInput("cmp4 column payload is too large"))?,
            value_count: chunk.metadata.value_count,
            null_count: chunk.metadata.null_count,
            statistics_metadata: chunk.metadata.statistics_metadata,
        });

        let written_metadata_len = &row_group[metadata_len_offset..metadata_len_offset + 4];
        debug_assert_eq!(written_metadata_len, metadata_len.to_le_bytes());
    }

    let checksum = crc32::checksum(&row_group);
    row_group.extend_from_slice(&checksum.to_le_bytes());
    let row_group_len = u64::try_from(row_group.len())
        .map_err(|_| CompactError::InvalidInput("cmp4 row group length is too large"))?;

    Ok((
        row_group,
        RowGroupIndexEntry {
            row_group_index: u64::try_from(row_group_index)
                .map_err(|_| CompactError::InvalidInput("cmp4 row group index is too large"))?,
            first_row_index: u64::try_from(first_row_index)
                .map_err(|_| CompactError::InvalidInput("cmp4 first row index is too large"))?,
            row_count,
            row_group_offset,
            row_group_len,
            columns: column_entries,
        },
    ))
}

fn decode_row_group_projection(
    data: &[u8],
    row_group: &RowGroupIndexEntry,
    schema_columns: &[ColumnSchema],
    projected_columns: &[&ColumnSchema],
    predicate: Option<&Predicate>,
    verify_full_checksum: bool,
) -> Result<Vec<DecodedSelection>> {
    verify_row_group(data, row_group, verify_full_checksum)?;

    let mut selected =
        Vec::with_capacity(projected_columns.len() + usize::from(predicate.is_some()));
    let mut required_names = projected_columns
        .iter()
        .map(|column| column.name.as_str())
        .collect::<Vec<_>>();
    if let Some(predicate_column) = predicate_column(predicate)
        && !required_names.contains(&predicate_column)
    {
        required_names.push(predicate_column);
    }

    for name in required_names {
        let schema = projected_columns
            .iter()
            .copied()
            .find(|column| column.name == name)
            .or_else(|| schema_columns.iter().find(|column| column.name == name));
        let schema = schema.ok_or(CompactError::InvalidInput(
            "cmp4 projected column does not match schema",
        ))?;
        let footer_column = row_group
            .columns
            .iter()
            .find(|column| column.name == name)
            .ok_or(CompactError::InvalidInput(
                "cmp4 projected column is missing from row group",
            ))?;
        let decoded = decode_footer_column(data, row_group, footer_column, schema)?;
        selected.push(DecodedSelection {
            name: schema.name.clone(),
            decoded,
            render: projected_columns
                .iter()
                .any(|column| column.name == schema.name),
        });
    }

    Ok(selected)
}

fn decode_footer_column(
    data: &[u8],
    row_group: &RowGroupIndexEntry,
    footer_column: &ColumnIndexEntry,
    schema: &ColumnSchema,
) -> Result<DecodedColumn> {
    let metadata_bytes = checked_slice(
        data,
        footer_column.metadata_offset,
        footer_column.metadata_len,
        "cmp4 column metadata is truncated",
    )?;
    let (metadata, consumed) = decode_column_metadata(metadata_bytes)?;
    if consumed != metadata_bytes.len() {
        return Err(CompactError::InvalidInput(
            "cmp4 column metadata has trailing bytes",
        ));
    }
    validate_metadata_against_schema(
        &metadata,
        schema,
        usize::try_from(row_group.row_count)
            .map_err(|_| CompactError::InvalidInput("cmp4 row count is too large"))?,
    )?;
    validate_footer_column_matches_metadata(footer_column, &metadata)?;
    let payload = checked_slice(
        data,
        footer_column.payload_offset,
        footer_column.payload_len,
        "cmp4 column payload is truncated",
    )?;

    decode_column(&metadata, payload)
}

fn validate_footer_column_matches_metadata(
    footer_column: &ColumnIndexEntry,
    metadata: &ColumnChunkMetadata,
) -> Result<()> {
    if footer_column.name != metadata.name
        || footer_column.value_count != metadata.value_count
        || footer_column.null_count != metadata.null_count
        || footer_column.statistics_metadata != metadata.statistics_metadata
        || footer_column.payload_len != metadata.compressed_size
    {
        return Err(CompactError::InvalidInput(
            "cmp4 footer column does not match metadata",
        ));
    }

    Ok(())
}

fn render_filtered_rows(
    out: &mut String,
    projected_columns: &[&ColumnSchema],
    decoded: &[DecodedSelection],
    predicate: Option<&Predicate>,
) -> Result<()> {
    let row_count = decoded
        .first()
        .map(|column| column.decoded_len())
        .unwrap_or(0);

    for row_index in 0..row_count {
        if !row_matches(decoded, row_index, predicate)? {
            continue;
        }

        out.push('{');
        for (index, column) in projected_columns.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let decoded_column = decoded
                .iter()
                .find(|decoded| decoded.render && decoded.name == column.name)
                .ok_or(CompactError::InvalidInput(
                    "cmp4 projected column was not decoded",
                ))?;
            out.push_str(
                &serde_json::to_string(&column.name)
                    .map_err(|_| CompactError::InvalidInput("json key cannot be serialized"))?,
            );
            out.push(':');
            out.push_str(&decoded_column.decoded.value_at(row_index)?.to_string());
        }
        out.push('}');
        out.push('\n');
    }

    Ok(())
}

fn row_matches(
    decoded: &[DecodedSelection],
    row_index: usize,
    predicate: Option<&Predicate>,
) -> Result<bool> {
    let Some(predicate) = predicate else {
        return Ok(true);
    };
    let column_name = predicate_column(Some(predicate)).expect("predicate has a column");
    let value = decoded
        .iter()
        .find(|column| column.name == column_name)
        .ok_or(CompactError::InvalidInput(
            "cmp4 predicate column was not decoded",
        ))?
        .decoded
        .value_at(row_index)?;

    match predicate {
        Predicate::U64 { op, .. } => {
            let Some(value) = value.as_u64() else {
                return Ok(false);
            };
            Ok(match op {
                U64PredicateOp::Eq(expected) => value == *expected,
                U64PredicateOp::Lt(expected) => value < *expected,
                U64PredicateOp::Le(expected) => value <= *expected,
                U64PredicateOp::Gt(expected) => value > *expected,
                U64PredicateOp::Ge(expected) => value >= *expected,
            })
        }
        Predicate::IsNull { .. } => Ok(value.is_null()),
    }
}

fn row_group_may_match(
    row_group: &RowGroupIndexEntry,
    predicate: Option<&Predicate>,
) -> Result<bool> {
    let Some(predicate) = predicate else {
        return Ok(true);
    };
    let column_name = predicate_column(Some(predicate)).expect("predicate has a column");
    let footer_column = row_group
        .columns
        .iter()
        .find(|column| column.name == column_name)
        .ok_or(CompactError::InvalidInput(
            "cmp4 predicate column is missing from row group",
        ))?;

    match predicate {
        Predicate::IsNull { .. } => Ok(footer_column.null_count > 0),
        Predicate::U64 { op, .. } => {
            let statistics = crate::statistics::decode(&footer_column.statistics_metadata)?;
            let ColumnStatistics::U64 { min, max } = statistics else {
                return Ok(true);
            };
            Ok(match op {
                U64PredicateOp::Eq(value) => *value >= min && *value <= max,
                U64PredicateOp::Lt(value) => min < *value,
                U64PredicateOp::Le(value) => min <= *value,
                U64PredicateOp::Gt(value) => max > *value,
                U64PredicateOp::Ge(value) => max >= *value,
            })
        }
    }
}

fn select_projection<'a>(
    columns: &'a [ColumnSchema],
    projection: &[&str],
) -> Result<Vec<&'a ColumnSchema>> {
    if projection.is_empty() {
        return Ok(columns.iter().collect());
    }

    let mut seen = HashSet::new();
    let mut selected = Vec::with_capacity(projection.len());
    for name in projection {
        if !seen.insert(*name) {
            return Err(CompactError::InvalidInput(
                "cmp4 projection contains duplicate column",
            ));
        }
        let column = columns.iter().find(|column| column.name == *name).ok_or(
            CompactError::InvalidInput("cmp4 projection column does not exist"),
        )?;
        selected.push(column);
    }

    Ok(selected)
}

fn validate_predicate(columns: &[ColumnSchema], predicate: Option<&Predicate>) -> Result<()> {
    let Some(predicate) = predicate else {
        return Ok(());
    };
    let column_name = predicate_column(Some(predicate)).expect("predicate has a column");
    let column = columns
        .iter()
        .find(|column| column.name == column_name)
        .ok_or(CompactError::InvalidInput(
            "cmp4 predicate column does not exist",
        ))?;

    match predicate {
        Predicate::U64 { .. } if column.value_type != SchemaValueType::U64 => Err(
            CompactError::InvalidInput("cmp4 u64 predicate requires a u64 column"),
        ),
        _ => Ok(()),
    }
}

fn predicate_column(predicate: Option<&Predicate>) -> Option<&str> {
    match predicate? {
        Predicate::U64 { column, .. } | Predicate::IsNull { column } => Some(column.as_str()),
    }
}

fn verify_row_group(
    data: &[u8],
    row_group: &RowGroupIndexEntry,
    verify_full_checksum: bool,
) -> Result<()> {
    const ROW_GROUP_HEADER_LEN: usize = 4 + 8 + 8 + 8 + 8 + 4;

    if row_group.row_group_len
        < u64::try_from(ROW_GROUP_HEADER_LEN + CHECKSUM_LEN)
            .expect("row group header length fits in u64")
    {
        return Err(CompactError::InvalidInput("cmp4 row group is truncated"));
    }

    let header = checked_slice(
        data,
        row_group.row_group_offset,
        ROW_GROUP_HEADER_LEN as u64,
        "cmp4 row group header is truncated",
    )?;

    if verify_full_checksum {
        let bytes = checked_slice(
            data,
            row_group.row_group_offset,
            row_group.row_group_len,
            "cmp4 row group is truncated",
        )?;
        let checksum_offset = bytes.len() - CHECKSUM_LEN;
        let stored_checksum = u32::from_le_bytes(
            bytes[checksum_offset..]
                .try_into()
                .expect("checksum suffix contains four bytes"),
        );
        if !crc32::verify(&bytes[..checksum_offset], stored_checksum) {
            return Err(CompactError::InvalidInput(
                "cmp4 row group checksum mismatch",
            ));
        }
    }

    let mut cursor = 0usize;
    if read_exact(
        header,
        &mut cursor,
        ROW_GROUP_MAGIC.len(),
        "cmp4 row group magic is truncated",
    )? != ROW_GROUP_MAGIC
    {
        return Err(CompactError::InvalidInput("invalid cmp4 row group magic"));
    }
    if read_u64(header, &mut cursor, "cmp4 row group index is truncated")?
        != row_group.row_group_index
    {
        return Err(CompactError::InvalidInput(
            "cmp4 row group index does not match footer",
        ));
    }
    if read_u64(header, &mut cursor, "cmp4 first row index is truncated")?
        != row_group.first_row_index
    {
        return Err(CompactError::InvalidInput(
            "cmp4 first row index does not match footer",
        ));
    }
    if read_u64(header, &mut cursor, "cmp4 row count is truncated")? != row_group.row_count {
        return Err(CompactError::InvalidInput(
            "cmp4 row count does not match footer",
        ));
    }
    read_u64(header, &mut cursor, "cmp4 raw jsonl size is truncated")?;
    if read_u32(header, &mut cursor, "cmp4 column count is truncated")? as usize
        != row_group.columns.len()
    {
        return Err(CompactError::InvalidInput(
            "cmp4 column count does not match footer",
        ));
    }

    Ok(())
}

fn checked_slice<'a>(data: &'a [u8], offset: u64, len: u64, err: &'static str) -> Result<&'a [u8]> {
    let start = usize::try_from(offset)
        .map_err(|_| CompactError::InvalidInput("cmp4 offset is too large"))?;
    let len =
        usize::try_from(len).map_err(|_| CompactError::InvalidInput("cmp4 length is too large"))?;
    let end = start
        .checked_add(len)
        .ok_or(CompactError::InvalidInput("cmp4 range overflow"))?;

    data.get(start..end).ok_or(CompactError::InvalidInput(err))
}

fn absolute_offset(base: u64, relative: usize) -> Result<u64> {
    base.checked_add(
        u64::try_from(relative)
            .map_err(|_| CompactError::InvalidInput("cmp4 relative offset is too large"))?,
    )
    .ok_or(CompactError::InvalidInput("cmp4 offset overflow"))
}

fn read_exact<'a>(
    data: &'a [u8],
    cursor: &mut usize,
    len: usize,
    err: &'static str,
) -> Result<&'a [u8]> {
    let end = cursor
        .checked_add(len)
        .ok_or(CompactError::InvalidInput("cmp4 row group length overflow"))?;
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

struct DecodedSelection {
    name: String,
    decoded: DecodedColumn,
    render: bool,
}

impl DecodedSelection {
    fn decoded_len(&self) -> usize {
        match &self.decoded {
            DecodedColumn::Bool(values) => values.len(),
            DecodedColumn::U64(values) => values.len(),
            DecodedColumn::String(values) => values.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EncodeOptions, Predicate, U64PredicateOp, decode_jsonl, decode_jsonl_projected,
        encode_jsonl, inspect_footer, recover_file_prefix, recover_row_group, scan_jsonl,
        validate_file,
    };
    use crate::CompactError;
    use crate::schema::Schema;

    fn schema() -> Schema {
        Schema::from_yaml(
            r#"
columns:
  - name: id
    type: u64
    codec: delta_bitpack
  - name: active
    type: bool
    codec: bitmap
  - name: service
    type: string
    codec: prefix
    nullable: true
"#,
        )
        .unwrap()
    }

    fn input() -> &'static str {
        "{\"id\":1,\"active\":true,\"service\":\"api\"}\n{\"id\":2,\"active\":false,\"service\":\"api\"}\n{\"id\":10,\"active\":true,\"service\":null}\n{\"id\":11,\"active\":false,\"service\":\"worker\"}\n"
    }

    #[test]
    fn cmp4_multi_row_group_roundtrips() {
        let encoded =
            encode_jsonl(input(), &schema(), EncodeOptions { row_group_rows: 2 }).unwrap();
        let decoded = decode_jsonl(&encoded, &schema()).unwrap();

        assert_eq!(decoded, input());
    }

    #[test]
    fn cmp4_file_validator_checks_all_row_group_checksums() {
        let mut encoded =
            encode_jsonl(input(), &schema(), EncodeOptions { row_group_rows: 2 }).unwrap();
        let footer = inspect_footer(&encoded).unwrap();
        let payload_offset = footer.row_groups[1].columns[0].payload_offset as usize;

        assert_eq!(validate_file(&encoded).unwrap().total_row_count, 4);

        encoded[payload_offset] ^= 0xff;
        let error = validate_file(&encoded).unwrap_err();
        assert!(matches!(
            error,
            CompactError::InvalidInput("cmp4 row group checksum mismatch")
        ));
    }

    #[test]
    fn cmp4_recovery_reconstructs_footer_after_damaged_trailer() {
        let mut encoded =
            encode_jsonl(input(), &schema(), EncodeOptions { row_group_rows: 2 }).unwrap();
        *encoded.last_mut().unwrap() ^= 0xff;

        let recovery = recover_file_prefix(&encoded).unwrap();

        assert_eq!(recovery.footer.row_groups.len(), 2);
        assert_eq!(recovery.footer.total_row_count, 4);
        assert!(recovery.discarded_tail);
    }

    #[test]
    fn cmp4_recovery_stops_before_corrupt_row_group() {
        let mut encoded =
            encode_jsonl(input(), &schema(), EncodeOptions { row_group_rows: 2 }).unwrap();
        let footer = inspect_footer(&encoded).unwrap();
        let corrupt_at = footer.row_groups[1].columns[0].payload_offset as usize;
        encoded[corrupt_at] ^= 0xff;

        let recovery = recover_file_prefix(&encoded).unwrap();

        assert_eq!(recovery.footer.row_groups.len(), 1);
        assert_eq!(recovery.footer.total_row_count, 2);
        assert_eq!(
            recovery.valid_body_len,
            footer.row_groups[1].row_group_offset
        );
    }

    #[test]
    fn cmp4_recovery_rejects_column_count_beyond_remaining_bytes() {
        let mut encoded =
            encode_jsonl(input(), &schema(), EncodeOptions { row_group_rows: 2 }).unwrap();
        let column_count_offset = 10 + 4 + 4 * 8;
        encoded[column_count_offset..column_count_offset + 4]
            .copy_from_slice(&u32::MAX.to_le_bytes());

        assert!(matches!(
            recover_row_group(&encoded, 10, 0, 0).unwrap_err(),
            CompactError::InvalidInput(_)
        ));
    }

    #[test]
    fn cmp4_projection_decodes_only_selected_columns() {
        let encoded =
            encode_jsonl(input(), &schema(), EncodeOptions { row_group_rows: 2 }).unwrap();
        let decoded = decode_jsonl_projected(&encoded, &schema(), &["id", "service"]).unwrap();

        assert_eq!(
            decoded,
            "{\"id\":1,\"service\":\"api\"}\n{\"id\":2,\"service\":\"api\"}\n{\"id\":10,\"service\":null}\n{\"id\":11,\"service\":\"worker\"}\n"
        );
    }

    #[test]
    fn cmp4_projection_authenticates_unselected_payloads() {
        let mut encoded =
            encode_jsonl(input(), &schema(), EncodeOptions { row_group_rows: 2 }).unwrap();
        let footer = inspect_footer(&encoded).unwrap();
        let active_payload_offset = footer.row_groups[0].columns[1].payload_offset as usize;
        encoded[active_payload_offset] ^= 0xff;

        let err = decode_jsonl_projected(&encoded, &schema(), &["id", "service"]).unwrap_err();
        assert!(matches!(
            err,
            CompactError::InvalidInput("cmp4 row group checksum mismatch")
        ));
    }

    #[test]
    fn cmp4_predicate_pushdown_prunes_row_groups_and_filters_rows() {
        let encoded =
            encode_jsonl(input(), &schema(), EncodeOptions { row_group_rows: 2 }).unwrap();
        let result = scan_jsonl(
            &encoded,
            &schema(),
            &["id"],
            Some(&Predicate::U64 {
                column: "id".to_owned(),
                op: U64PredicateOp::Ge(10),
            }),
        )
        .unwrap();

        assert_eq!(result.row_groups_scanned, 1);
        assert_eq!(result.row_groups_pruned, 1);
        assert_eq!(result.jsonl, "{\"id\":10}\n{\"id\":11}\n");
    }

    #[test]
    fn cmp4_is_null_predicate_uses_null_count_for_pruning() {
        let encoded =
            encode_jsonl(input(), &schema(), EncodeOptions { row_group_rows: 2 }).unwrap();
        let result = scan_jsonl(
            &encoded,
            &schema(),
            &["id", "service"],
            Some(&Predicate::IsNull {
                column: "service".to_owned(),
            }),
        )
        .unwrap();

        assert_eq!(result.row_groups_scanned, 1);
        assert_eq!(result.row_groups_pruned, 1);
        assert_eq!(result.jsonl, "{\"id\":10,\"service\":null}\n");
    }

    #[test]
    fn cmp4_footer_inspect_reads_row_group_and_column_ranges() {
        let encoded =
            encode_jsonl(input(), &schema(), EncodeOptions { row_group_rows: 2 }).unwrap();
        let footer = inspect_footer(&encoded).unwrap();

        assert_eq!(footer.total_row_count, 4);
        assert_eq!(footer.row_groups.len(), 2);
        assert_eq!(footer.row_groups[0].columns.len(), 3);
        assert_eq!(footer.row_groups[0].columns[0].name, "id");
    }

    #[test]
    fn cmp4_rejects_unknown_projection_column() {
        let encoded =
            encode_jsonl(input(), &schema(), EncodeOptions { row_group_rows: 2 }).unwrap();
        let err = decode_jsonl_projected(&encoded, &schema(), &["missing"]).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("cmp4 projection column does not exist")
        ));
    }

    #[test]
    fn cmp4_rejects_zero_row_group_limit() {
        let err =
            encode_jsonl(input(), &schema(), EncodeOptions { row_group_rows: 0 }).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("cmp4 row group row limit must be positive")
        ));
    }
}
