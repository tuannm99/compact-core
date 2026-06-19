//! Binary metadata foundation for the CMP4 queryable columnar format.
//!
//! CMP4 keeps the CMP3 row-group payload model, then adds an EOF footer index so
//! readers can inspect metadata, find row groups by row number, and locate a
//! projected column without scanning every payload. Query execution is added in
//! later phases; this module owns the stable on-disk contract those phases use.

use std::collections::HashSet;

use crate::{CompactError, MAGIC_V4, Result, VERSION_V4, checksum32};

const FILE_HEADER_LEN: usize = 4 + 1 + 1 + 4;
const FOOTER_MAGIC: [u8; 4] = *b"IDX4";
const FOOTER_TRAILER_LEN: usize = 8 + 8 + 4 + 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHeader {
    pub flags: u8,
    pub payload: Vec<u8>,
    pub body_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FooterIndex {
    pub total_row_count: u64,
    pub row_groups: Vec<RowGroupIndexEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowGroupIndexEntry {
    pub row_group_index: u64,
    pub first_row_index: u64,
    pub row_count: u64,
    pub row_group_offset: u64,
    pub row_group_len: u64,
    pub columns: Vec<ColumnIndexEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnIndexEntry {
    pub name: String,
    pub metadata_offset: u64,
    pub metadata_len: u64,
    pub payload_offset: u64,
    pub payload_len: u64,
    pub value_count: u64,
    pub null_count: u64,
    pub statistics_metadata: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FooterTrailer {
    pub footer_offset: u64,
    pub footer_len: u64,
    pub footer_checksum: u32,
}

/// Encode an empty CMP4 header with no optional header payload.
pub fn encode_empty_header() -> Vec<u8> {
    let mut out = Vec::with_capacity(FILE_HEADER_LEN);
    out.extend_from_slice(&MAGIC_V4);
    out.push(VERSION_V4);
    out.push(0);
    out.extend_from_slice(&0u32.to_le_bytes());
    out
}

/// Parse and validate the CMP4 file header.
pub fn decode_header(data: &[u8]) -> Result<FileHeader> {
    if data.len() < FILE_HEADER_LEN {
        return Err(CompactError::InvalidInput("cmp4 header is truncated"));
    }

    if data[..4] != MAGIC_V4 {
        return Err(CompactError::InvalidInput("invalid cmp4 magic"));
    }

    if data[4] != VERSION_V4 {
        return Err(CompactError::Unsupported("cmp4 version"));
    }

    let flags = data[5];
    if flags != 0 {
        return Err(CompactError::Unsupported("cmp4 flags"));
    }

    let payload_len = u32::from_le_bytes(
        data[6..10]
            .try_into()
            .expect("fixed header offsets provide four bytes"),
    ) as usize;
    let body_offset = FILE_HEADER_LEN
        .checked_add(payload_len)
        .ok_or(CompactError::InvalidInput("cmp4 header length overflow"))?;
    if body_offset > data.len() {
        return Err(CompactError::InvalidInput(
            "cmp4 header payload is truncated",
        ));
    }

    Ok(FileHeader {
        flags,
        payload: data[FILE_HEADER_LEN..body_offset].to_vec(),
        body_offset,
    })
}

/// Encode footer index bytes without the EOF trailer.
pub fn encode_footer(index: &FooterIndex) -> Result<Vec<u8>> {
    validate_footer_index(index, None)?;

    let row_group_count = u64::try_from(index.row_groups.len())
        .map_err(|_| CompactError::InvalidInput("cmp4 row group count is too large"))?;
    let mut out = Vec::new();
    out.extend_from_slice(&index.total_row_count.to_le_bytes());
    out.extend_from_slice(&row_group_count.to_le_bytes());

    for row_group in &index.row_groups {
        let column_count = u32::try_from(row_group.columns.len())
            .map_err(|_| CompactError::InvalidInput("cmp4 column count is too large"))?;
        out.extend_from_slice(&row_group.row_group_index.to_le_bytes());
        out.extend_from_slice(&row_group.first_row_index.to_le_bytes());
        out.extend_from_slice(&row_group.row_count.to_le_bytes());
        out.extend_from_slice(&row_group.row_group_offset.to_le_bytes());
        out.extend_from_slice(&row_group.row_group_len.to_le_bytes());
        out.extend_from_slice(&column_count.to_le_bytes());

        for column in &row_group.columns {
            let name = column.name.as_bytes();
            let name_len = u16::try_from(name.len())
                .map_err(|_| CompactError::InvalidInput("cmp4 column name is too long"))?;
            let statistics_len = u32::try_from(column.statistics_metadata.len())
                .map_err(|_| CompactError::InvalidInput("cmp4 statistics metadata is too large"))?;
            out.extend_from_slice(&name_len.to_le_bytes());
            out.extend_from_slice(name);
            out.extend_from_slice(&column.metadata_offset.to_le_bytes());
            out.extend_from_slice(&column.metadata_len.to_le_bytes());
            out.extend_from_slice(&column.payload_offset.to_le_bytes());
            out.extend_from_slice(&column.payload_len.to_le_bytes());
            out.extend_from_slice(&column.value_count.to_le_bytes());
            out.extend_from_slice(&column.null_count.to_le_bytes());
            out.extend_from_slice(&statistics_len.to_le_bytes());
            out.extend_from_slice(&column.statistics_metadata);
        }
    }

    Ok(out)
}

/// Append a checked footer and fixed-width EOF trailer to an encoded CMP4 file.
pub fn append_footer(file: &mut Vec<u8>, index: &FooterIndex) -> Result<FooterTrailer> {
    let footer_offset = u64::try_from(file.len())
        .map_err(|_| CompactError::InvalidInput("cmp4 file length is too large"))?;
    validate_footer_index(index, Some(footer_offset))?;

    let footer = encode_footer(index)?;
    let footer_len = u64::try_from(footer.len())
        .map_err(|_| CompactError::InvalidInput("cmp4 footer length is too large"))?;
    let footer_checksum = checksum32(&footer);

    file.extend_from_slice(&footer);
    file.extend_from_slice(&footer_offset.to_le_bytes());
    file.extend_from_slice(&footer_len.to_le_bytes());
    file.extend_from_slice(&footer_checksum.to_le_bytes());
    file.extend_from_slice(&FOOTER_MAGIC);

    Ok(FooterTrailer {
        footer_offset,
        footer_len,
        footer_checksum,
    })
}

/// Decode the CMP4 footer by reading only the fixed-width EOF trailer first.
pub fn decode_footer(file: &[u8]) -> Result<FooterIndex> {
    let trailer = decode_footer_trailer(file)?;
    let footer_start = usize::try_from(trailer.footer_offset)
        .map_err(|_| CompactError::InvalidInput("cmp4 footer offset is too large"))?;
    let footer_len = usize::try_from(trailer.footer_len)
        .map_err(|_| CompactError::InvalidInput("cmp4 footer length is too large"))?;
    let footer_end = footer_start
        .checked_add(footer_len)
        .ok_or(CompactError::InvalidInput("cmp4 footer range overflow"))?;
    let trailer_start =
        file.len()
            .checked_sub(FOOTER_TRAILER_LEN)
            .ok_or(CompactError::InvalidInput(
                "cmp4 footer trailer is truncated",
            ))?;

    if footer_end != trailer_start {
        return Err(CompactError::InvalidInput("cmp4 footer range is invalid"));
    }

    let footer = &file[footer_start..footer_end];
    if checksum32(footer) != trailer.footer_checksum {
        return Err(CompactError::InvalidInput("cmp4 footer checksum mismatch"));
    }

    let mut cursor = 0usize;
    let total_row_count = read_u64(footer, &mut cursor, "cmp4 total row count is truncated")?;
    let row_group_count = read_u64(footer, &mut cursor, "cmp4 row group count is truncated")?;
    let mut row_groups = Vec::with_capacity(
        usize::try_from(row_group_count)
            .map_err(|_| CompactError::InvalidInput("cmp4 row group count is too large"))?,
    );

    for _ in 0..row_group_count {
        let row_group_index = read_u64(footer, &mut cursor, "cmp4 row group index is truncated")?;
        let first_row_index = read_u64(footer, &mut cursor, "cmp4 first row index is truncated")?;
        let row_count = read_u64(footer, &mut cursor, "cmp4 row count is truncated")?;
        let row_group_offset = read_u64(footer, &mut cursor, "cmp4 row group offset is truncated")?;
        let row_group_len = read_u64(footer, &mut cursor, "cmp4 row group length is truncated")?;
        let column_count = read_u32(footer, &mut cursor, "cmp4 column count is truncated")?;
        let mut columns = Vec::with_capacity(
            usize::try_from(column_count)
                .map_err(|_| CompactError::InvalidInput("cmp4 column count is too large"))?,
        );

        for _ in 0..column_count {
            let name_len =
                read_u16(footer, &mut cursor, "cmp4 column name length is truncated")? as usize;
            let name_bytes = read_exact(
                footer,
                &mut cursor,
                name_len,
                "cmp4 column name is truncated",
            )?;
            let name = std::str::from_utf8(name_bytes)
                .map_err(|_| CompactError::InvalidInput("cmp4 column name must be utf-8"))?
                .to_owned();
            let metadata_offset =
                read_u64(footer, &mut cursor, "cmp4 metadata offset is truncated")?;
            let metadata_len = read_u64(footer, &mut cursor, "cmp4 metadata length is truncated")?;
            let payload_offset = read_u64(footer, &mut cursor, "cmp4 payload offset is truncated")?;
            let payload_len = read_u64(footer, &mut cursor, "cmp4 payload length is truncated")?;
            let value_count = read_u64(footer, &mut cursor, "cmp4 value count is truncated")?;
            let null_count = read_u64(footer, &mut cursor, "cmp4 null count is truncated")?;
            let statistics_len =
                read_u32(footer, &mut cursor, "cmp4 statistics length is truncated")? as usize;
            let statistics_metadata = read_exact(
                footer,
                &mut cursor,
                statistics_len,
                "cmp4 statistics metadata is truncated",
            )?
            .to_vec();

            columns.push(ColumnIndexEntry {
                name,
                metadata_offset,
                metadata_len,
                payload_offset,
                payload_len,
                value_count,
                null_count,
                statistics_metadata,
            });
        }

        row_groups.push(RowGroupIndexEntry {
            row_group_index,
            first_row_index,
            row_count,
            row_group_offset,
            row_group_len,
            columns,
        });
    }

    if cursor != footer.len() {
        return Err(CompactError::InvalidInput("cmp4 footer has trailing bytes"));
    }

    let index = FooterIndex {
        total_row_count,
        row_groups,
    };
    validate_footer_index(&index, Some(trailer.footer_offset))?;

    Ok(index)
}

/// Locate the row group that owns `row_number` with binary search.
pub fn find_row_group_by_row(index: &FooterIndex, row_number: u64) -> Option<&RowGroupIndexEntry> {
    let position = index
        .row_groups
        .partition_point(|row_group| row_group.first_row_index <= row_number);
    let row_group = index.row_groups.get(position.checked_sub(1)?)?;
    let row_group_end = row_group.first_row_index.checked_add(row_group.row_count)?;

    (row_number < row_group_end).then_some(row_group)
}

fn decode_footer_trailer(file: &[u8]) -> Result<FooterTrailer> {
    if file.len() < FOOTER_TRAILER_LEN {
        return Err(CompactError::InvalidInput(
            "cmp4 footer trailer is truncated",
        ));
    }

    let trailer_start = file.len() - FOOTER_TRAILER_LEN;
    let mut cursor = trailer_start;
    let footer_offset = read_u64(file, &mut cursor, "cmp4 footer offset is truncated")?;
    let footer_len = read_u64(file, &mut cursor, "cmp4 footer length is truncated")?;
    let footer_checksum = read_u32(file, &mut cursor, "cmp4 footer checksum is truncated")?;
    let magic = read_exact(
        file,
        &mut cursor,
        FOOTER_MAGIC.len(),
        "cmp4 footer magic is truncated",
    )?;
    if magic != FOOTER_MAGIC {
        return Err(CompactError::InvalidInput("invalid cmp4 footer magic"));
    }

    Ok(FooterTrailer {
        footer_offset,
        footer_len,
        footer_checksum,
    })
}

fn validate_footer_index(index: &FooterIndex, footer_offset: Option<u64>) -> Result<()> {
    let mut expected_first_row = 0u64;

    for (position, row_group) in index.row_groups.iter().enumerate() {
        if row_group.row_group_index != position as u64 {
            return Err(CompactError::InvalidInput(
                "cmp4 row group indexes must be contiguous",
            ));
        }

        if row_group.row_count == 0 {
            return Err(CompactError::InvalidInput(
                "cmp4 row group must contain rows",
            ));
        }

        if row_group.first_row_index != expected_first_row {
            return Err(CompactError::InvalidInput(
                "cmp4 row groups must be sorted and contiguous",
            ));
        }

        expected_first_row = expected_first_row
            .checked_add(row_group.row_count)
            .ok_or(CompactError::InvalidInput("cmp4 total row count overflow"))?;
        let row_group_end = checked_range_end(
            row_group.row_group_offset,
            row_group.row_group_len,
            "cmp4 row group range overflow",
        )?;
        ensure_before_footer(
            row_group_end,
            footer_offset,
            "cmp4 row group range is invalid",
        )?;

        let mut names = HashSet::new();
        for column in &row_group.columns {
            validate_column_index(column)?;
            if !names.insert(column.name.as_str()) {
                return Err(CompactError::InvalidInput(
                    "cmp4 duplicate column in row group",
                ));
            }

            let metadata_end = checked_range_end(
                column.metadata_offset,
                column.metadata_len,
                "cmp4 metadata range overflow",
            )?;
            let payload_end = checked_range_end(
                column.payload_offset,
                column.payload_len,
                "cmp4 payload range overflow",
            )?;
            ensure_range_inside_row_group(
                column.metadata_offset,
                metadata_end,
                row_group,
                "cmp4 metadata range is outside row group",
            )?;
            ensure_range_inside_row_group(
                column.payload_offset,
                payload_end,
                row_group,
                "cmp4 payload range is outside row group",
            )?;
        }
    }

    if expected_first_row != index.total_row_count {
        return Err(CompactError::InvalidInput(
            "cmp4 total row count does not match row groups",
        ));
    }

    Ok(())
}

fn validate_column_index(column: &ColumnIndexEntry) -> Result<()> {
    if column.name.is_empty() {
        return Err(CompactError::InvalidInput(
            "cmp4 column name must not be empty",
        ));
    }

    if column.null_count > column.value_count {
        return Err(CompactError::InvalidInput(
            "cmp4 null count exceeds value count",
        ));
    }

    Ok(())
}

fn checked_range_end(offset: u64, len: u64, err: &'static str) -> Result<u64> {
    offset
        .checked_add(len)
        .ok_or(CompactError::InvalidInput(err))
}

fn ensure_before_footer(end: u64, footer_offset: Option<u64>, err: &'static str) -> Result<()> {
    if footer_offset.is_some_and(|offset| end > offset) {
        return Err(CompactError::InvalidInput(err));
    }

    Ok(())
}

fn ensure_range_inside_row_group(
    start: u64,
    end: u64,
    row_group: &RowGroupIndexEntry,
    err: &'static str,
) -> Result<()> {
    let row_group_start = row_group.row_group_offset;
    let row_group_end = row_group
        .row_group_offset
        .checked_add(row_group.row_group_len)
        .ok_or(CompactError::InvalidInput("cmp4 row group range overflow"))?;

    if start < row_group_start || end > row_group_end {
        return Err(CompactError::InvalidInput(err));
    }

    Ok(())
}

fn read_exact<'a>(
    data: &'a [u8],
    cursor: &mut usize,
    len: usize,
    err: &'static str,
) -> Result<&'a [u8]> {
    let end = cursor
        .checked_add(len)
        .ok_or(CompactError::InvalidInput("cmp4 metadata length overflow"))?;
    if end > data.len() {
        return Err(CompactError::InvalidInput(err));
    }

    let bytes = &data[*cursor..end];
    *cursor = end;
    Ok(bytes)
}

fn read_u16(data: &[u8], cursor: &mut usize, err: &'static str) -> Result<u16> {
    Ok(u16::from_le_bytes(
        read_exact(data, cursor, 2, err)?
            .try_into()
            .expect("read_exact returned two bytes"),
    ))
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
    use super::{
        ColumnIndexEntry, FooterIndex, RowGroupIndexEntry, append_footer, decode_footer,
        decode_header, encode_empty_header, encode_footer, find_row_group_by_row,
    };
    use crate::{CompactError, VERSION_V4, checksum32};

    fn column(name: &str, base: u64, value_count: u64) -> ColumnIndexEntry {
        ColumnIndexEntry {
            name: name.to_owned(),
            metadata_offset: base,
            metadata_len: 8,
            payload_offset: base + 8,
            payload_len: 16,
            value_count,
            null_count: 1,
            statistics_metadata: vec![1, 2, 3],
        }
    }

    fn index() -> FooterIndex {
        FooterIndex {
            total_row_count: 30,
            row_groups: vec![
                RowGroupIndexEntry {
                    row_group_index: 0,
                    first_row_index: 0,
                    row_count: 10,
                    row_group_offset: 10,
                    row_group_len: 100,
                    columns: vec![column("id", 20, 10), column("name", 50, 10)],
                },
                RowGroupIndexEntry {
                    row_group_index: 1,
                    first_row_index: 10,
                    row_count: 20,
                    row_group_offset: 110,
                    row_group_len: 100,
                    columns: vec![column("id", 120, 20), column("name", 150, 20)],
                },
            ],
        }
    }

    #[test]
    fn empty_cmp4_header_roundtrips() {
        let encoded = encode_empty_header();
        let decoded = decode_header(&encoded).unwrap();

        assert_eq!(decoded.flags, 0);
        assert!(decoded.payload.is_empty());
        assert_eq!(decoded.body_offset, encoded.len());
    }

    #[test]
    fn cmp4_header_rejects_unknown_version() {
        let mut encoded = encode_empty_header();
        encoded[4] = VERSION_V4 + 1;
        let err = decode_header(&encoded).unwrap_err();

        assert!(matches!(err, CompactError::Unsupported("cmp4 version")));
    }

    #[test]
    fn cmp4_header_rejects_unknown_flags() {
        let mut encoded = encode_empty_header();
        encoded[5] = 1;
        let err = decode_header(&encoded).unwrap_err();

        assert!(matches!(err, CompactError::Unsupported("cmp4 flags")));
    }

    #[test]
    fn cmp4_footer_roundtrips_from_eof_trailer() {
        let expected = index();
        let mut file = encode_empty_header();
        file.resize(210, 0);
        let trailer = append_footer(&mut file, &expected).unwrap();
        let decoded = decode_footer(&file).unwrap();

        assert_eq!(trailer.footer_offset, 210);
        assert_eq!(decoded, expected);
    }

    #[test]
    fn cmp4_row_group_lookup_uses_logical_row_ranges() {
        let index = index();

        assert_eq!(find_row_group_by_row(&index, 0).unwrap().row_group_index, 0);
        assert_eq!(find_row_group_by_row(&index, 9).unwrap().row_group_index, 0);
        assert_eq!(
            find_row_group_by_row(&index, 10).unwrap().row_group_index,
            1
        );
        assert_eq!(
            find_row_group_by_row(&index, 29).unwrap().row_group_index,
            1
        );
        assert!(find_row_group_by_row(&index, 30).is_none());
    }

    #[test]
    fn cmp4_footer_rejects_truncated_trailer() {
        let err = decode_footer(&[0; 8]).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("cmp4 footer trailer is truncated")
        ));
    }

    #[test]
    fn cmp4_footer_rejects_invalid_footer_magic() {
        let expected = index();
        let mut file = encode_empty_header();
        file.resize(210, 0);
        append_footer(&mut file, &expected).unwrap();
        *file.last_mut().unwrap() = b'X';
        let err = decode_footer(&file).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("invalid cmp4 footer magic")
        ));
    }

    #[test]
    fn cmp4_footer_rejects_checksum_mismatch() {
        let expected = index();
        let mut file = encode_empty_header();
        file.resize(210, 0);
        append_footer(&mut file, &expected).unwrap();
        file[210] ^= 0xff;
        let err = decode_footer(&file).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("cmp4 footer checksum mismatch")
        ));
    }

    #[test]
    fn cmp4_footer_rejects_duplicate_columns() {
        let mut invalid = index();
        invalid.row_groups[0].columns[1].name = "id".to_owned();
        let err = encode_footer(&invalid).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("cmp4 duplicate column in row group")
        ));
    }

    #[test]
    fn cmp4_footer_rejects_non_contiguous_rows() {
        let mut invalid = index();
        invalid.row_groups[1].first_row_index = 11;
        let err = encode_footer(&invalid).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("cmp4 row groups must be sorted and contiguous")
        ));
    }

    #[test]
    fn cmp4_footer_rejects_ranges_after_footer() {
        let mut invalid = index();
        invalid.row_groups[1].row_group_len = 200;
        let mut file = encode_empty_header();
        file.resize(210, 0);
        let err = append_footer(&mut file, &invalid).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("cmp4 row group range is invalid")
        ));
    }

    #[test]
    fn cmp4_footer_rejects_range_overflow() {
        let mut invalid = index();
        invalid.row_groups[0].columns[0].payload_offset = u64::MAX;
        let err = encode_footer(&invalid).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("cmp4 payload range overflow")
        ));
    }

    #[test]
    fn cmp4_footer_rejects_trailing_footer_bytes() {
        let expected = index();
        let mut file = encode_empty_header();
        file.resize(210, 0);
        let footer_offset = file.len() as u64;
        let mut footer = encode_footer(&expected).unwrap();
        footer.push(0xff);
        let footer_len = footer.len() as u64;
        let footer_checksum = checksum32(&footer);
        file.extend_from_slice(&footer);
        file.extend_from_slice(&footer_offset.to_le_bytes());
        file.extend_from_slice(&footer_len.to_le_bytes());
        file.extend_from_slice(&footer_checksum.to_le_bytes());
        file.extend_from_slice(b"IDX4");

        let err = decode_footer(&file).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("cmp4 footer has trailing bytes")
        ));
    }
}
