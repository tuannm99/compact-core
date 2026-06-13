//! Binary metadata foundation for the CMP3 column format.
//!
//! This module only defines and validates the v0.3 header and column chunk
//! metadata. Codec payload implementations are added in later phases. Keeping
//! metadata parsing independent makes malformed-input behavior testable before
//! new codecs are connected to JSONL encode/decode.

use crate::schema::{SchemaCodec, SchemaValueType};
use crate::{CompactError, MAGIC_V3, Result, VERSION_V3};

const FILE_HEADER_LEN: usize = 4 + 1 + 1 + 4;
const COLUMN_FIXED_LEN: usize = 1 + 1 + 1 + 8 + 8 + 8 + 8 + 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHeader {
    pub flags: u8,
    pub payload: Vec<u8>,
    pub body_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnChunkMetadata {
    pub name: String,
    pub value_type: SchemaValueType,
    pub nullable: bool,
    pub codec: SchemaCodec,
    pub value_count: u64,
    pub null_count: u64,
    pub raw_size: u64,
    pub compressed_size: u64,
    pub codec_metadata: Vec<u8>,
}

/// Encode an empty CMP3 header with no optional header payload.
pub fn encode_empty_header() -> Vec<u8> {
    let mut out = Vec::with_capacity(FILE_HEADER_LEN);
    out.extend_from_slice(&MAGIC_V3);
    out.push(VERSION_V3);
    out.push(0);
    out.extend_from_slice(&0u32.to_le_bytes());
    out
}

/// Parse and validate the CMP3 file header.
pub fn decode_header(data: &[u8]) -> Result<FileHeader> {
    if data.len() < FILE_HEADER_LEN {
        return Err(CompactError::InvalidInput("cmp3 header is truncated"));
    }

    if data[..4] != MAGIC_V3 {
        return Err(CompactError::InvalidInput("invalid cmp3 magic"));
    }

    if data[4] != VERSION_V3 {
        return Err(CompactError::Unsupported("cmp3 version"));
    }

    let flags = data[5];
    if flags != 0 {
        return Err(CompactError::Unsupported("cmp3 flags"));
    }

    let payload_len = u32::from_le_bytes(
        data[6..10]
            .try_into()
            .expect("fixed header offsets provide four bytes"),
    ) as usize;
    let body_offset = FILE_HEADER_LEN
        .checked_add(payload_len)
        .ok_or(CompactError::InvalidInput("cmp3 header length overflow"))?;
    if body_offset > data.len() {
        return Err(CompactError::InvalidInput(
            "cmp3 header payload is truncated",
        ));
    }

    Ok(FileHeader {
        flags,
        payload: data[FILE_HEADER_LEN..body_offset].to_vec(),
        body_offset,
    })
}

/// Encode one column chunk metadata record without its payload.
pub fn encode_column_metadata(metadata: &ColumnChunkMetadata) -> Result<Vec<u8>> {
    validate_column_metadata(metadata)?;

    let name = metadata.name.as_bytes();
    let name_len = u16::try_from(name.len())
        .map_err(|_| CompactError::InvalidInput("cmp3 column name is too long"))?;
    let codec_metadata_len = u32::try_from(metadata.codec_metadata.len())
        .map_err(|_| CompactError::InvalidInput("cmp3 codec metadata is too large"))?;
    let mut out =
        Vec::with_capacity(2 + name.len() + COLUMN_FIXED_LEN + metadata.codec_metadata.len());

    out.extend_from_slice(&name_len.to_le_bytes());
    out.extend_from_slice(name);
    out.push(value_type_to_id(metadata.value_type));
    out.push(u8::from(metadata.nullable));
    out.push(codec_to_id(metadata.codec));
    out.extend_from_slice(&metadata.value_count.to_le_bytes());
    out.extend_from_slice(&metadata.null_count.to_le_bytes());
    out.extend_from_slice(&metadata.raw_size.to_le_bytes());
    out.extend_from_slice(&metadata.compressed_size.to_le_bytes());
    out.extend_from_slice(&codec_metadata_len.to_le_bytes());
    out.extend_from_slice(&metadata.codec_metadata);

    Ok(out)
}

/// Decode one column metadata record and return the number of consumed bytes.
pub fn decode_column_metadata(data: &[u8]) -> Result<(ColumnChunkMetadata, usize)> {
    let mut cursor = 0usize;
    let name_len = read_u16(data, &mut cursor, "cmp3 column name length is truncated")? as usize;
    let name_bytes = read_exact(data, &mut cursor, name_len, "cmp3 column name is truncated")?;
    let name = std::str::from_utf8(name_bytes)
        .map_err(|_| CompactError::InvalidInput("cmp3 column name must be utf-8"))?
        .to_owned();
    let value_type = id_to_value_type(read_u8(
        data,
        &mut cursor,
        "cmp3 column value type is truncated",
    )?)?;
    let nullable = match read_u8(data, &mut cursor, "cmp3 nullable flag is truncated")? {
        0 => false,
        1 => true,
        _ => return Err(CompactError::InvalidInput("invalid cmp3 nullable flag")),
    };
    let codec = id_to_codec(read_u8(data, &mut cursor, "cmp3 codec id is truncated")?)?;
    let value_count = read_u64(data, &mut cursor, "cmp3 value count is truncated")?;
    let null_count = read_u64(data, &mut cursor, "cmp3 null count is truncated")?;
    let raw_size = read_u64(data, &mut cursor, "cmp3 raw size is truncated")?;
    let compressed_size = read_u64(data, &mut cursor, "cmp3 compressed size is truncated")?;
    let codec_metadata_len =
        read_u32(data, &mut cursor, "cmp3 codec metadata length is truncated")? as usize;
    let codec_metadata = read_exact(
        data,
        &mut cursor,
        codec_metadata_len,
        "cmp3 codec metadata is truncated",
    )?
    .to_vec();
    let metadata = ColumnChunkMetadata {
        name,
        value_type,
        nullable,
        codec,
        value_count,
        null_count,
        raw_size,
        compressed_size,
        codec_metadata,
    };

    validate_column_metadata(&metadata)?;

    Ok((metadata, cursor))
}

fn validate_column_metadata(metadata: &ColumnChunkMetadata) -> Result<()> {
    if metadata.name.is_empty() {
        return Err(CompactError::InvalidInput(
            "cmp3 column name must not be empty",
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

    let supported = match metadata.value_type {
        SchemaValueType::U64 => matches!(
            metadata.codec,
            SchemaCodec::Bitpack
                | SchemaCodec::DeltaBitpack
                | SchemaCodec::DeltaVarintU64
                | SchemaCodec::Stored
        ),
        SchemaValueType::String => matches!(
            metadata.codec,
            SchemaCodec::Dictionary | SchemaCodec::Prefix | SchemaCodec::Rle | SchemaCodec::Stored
        ),
        SchemaValueType::Bool => {
            matches!(metadata.codec, SchemaCodec::Bitmap | SchemaCodec::Stored)
        }
    };
    if !supported {
        return Err(CompactError::InvalidInput(
            "cmp3 selected codec does not match value type",
        ));
    }

    Ok(())
}

fn value_type_to_id(value_type: SchemaValueType) -> u8 {
    match value_type {
        SchemaValueType::Bool => 1,
        SchemaValueType::String => 2,
        SchemaValueType::U64 => 3,
    }
}

fn id_to_value_type(id: u8) -> Result<SchemaValueType> {
    match id {
        1 => Ok(SchemaValueType::Bool),
        2 => Ok(SchemaValueType::String),
        3 => Ok(SchemaValueType::U64),
        _ => Err(CompactError::Unsupported("cmp3 value type id")),
    }
}

fn codec_to_id(codec: SchemaCodec) -> u8 {
    match codec {
        SchemaCodec::Auto => 1,
        SchemaCodec::Bitmap => 2,
        SchemaCodec::Bitpack => 3,
        SchemaCodec::DeltaBitpack => 9,
        SchemaCodec::Dictionary => 4,
        SchemaCodec::DeltaVarintU64 => 5,
        SchemaCodec::Prefix => 6,
        SchemaCodec::Rle => 7,
        SchemaCodec::Stored => 8,
    }
}

fn id_to_codec(id: u8) -> Result<SchemaCodec> {
    match id {
        1 => Ok(SchemaCodec::Auto),
        2 => Ok(SchemaCodec::Bitmap),
        3 => Ok(SchemaCodec::Bitpack),
        9 => Ok(SchemaCodec::DeltaBitpack),
        4 => Ok(SchemaCodec::Dictionary),
        5 => Ok(SchemaCodec::DeltaVarintU64),
        6 => Ok(SchemaCodec::Prefix),
        7 => Ok(SchemaCodec::Rle),
        8 => Ok(SchemaCodec::Stored),
        _ => Err(CompactError::Unsupported("cmp3 codec id")),
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
        .ok_or(CompactError::InvalidInput("cmp3 metadata length overflow"))?;
    if end > data.len() {
        return Err(CompactError::InvalidInput(err));
    }

    let bytes = &data[*cursor..end];
    *cursor = end;
    Ok(bytes)
}

fn read_u8(data: &[u8], cursor: &mut usize, err: &'static str) -> Result<u8> {
    Ok(read_exact(data, cursor, 1, err)?[0])
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
        ColumnChunkMetadata, decode_column_metadata, decode_header, encode_column_metadata,
        encode_empty_header,
    };
    use crate::schema::{SchemaCodec, SchemaValueType};
    use crate::{CompactError, VERSION_V3};

    fn metadata() -> ColumnChunkMetadata {
        ColumnChunkMetadata {
            name: "active".to_owned(),
            value_type: SchemaValueType::Bool,
            nullable: true,
            codec: SchemaCodec::Bitmap,
            value_count: 100,
            null_count: 7,
            raw_size: 100,
            compressed_size: 25,
            codec_metadata: vec![93],
        }
    }

    #[test]
    fn empty_cmp3_header_roundtrips() {
        let encoded = encode_empty_header();
        let decoded = decode_header(&encoded).unwrap();

        assert_eq!(decoded.flags, 0);
        assert!(decoded.payload.is_empty());
        assert_eq!(decoded.body_offset, encoded.len());
    }

    #[test]
    fn cmp3_header_rejects_unknown_version() {
        let mut encoded = encode_empty_header();
        encoded[4] = VERSION_V3 + 1;
        let err = decode_header(&encoded).unwrap_err();

        assert!(matches!(err, CompactError::Unsupported("cmp3 version")));
    }

    #[test]
    fn cmp3_header_rejects_unknown_flags() {
        let mut encoded = encode_empty_header();
        encoded[5] = 1;
        let err = decode_header(&encoded).unwrap_err();

        assert!(matches!(err, CompactError::Unsupported("cmp3 flags")));
    }

    #[test]
    fn cmp3_header_rejects_truncated_fixed_header() {
        let encoded = encode_empty_header();
        let err = decode_header(&encoded[..encoded.len() - 1]).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("cmp3 header is truncated")
        ));
    }

    #[test]
    fn cmp3_header_rejects_truncated_payload() {
        let mut encoded = encode_empty_header();
        encoded[6..10].copy_from_slice(&1u32.to_le_bytes());
        let err = decode_header(&encoded).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("cmp3 header payload is truncated")
        ));
    }

    #[test]
    fn column_metadata_roundtrips() {
        let expected = metadata();
        let encoded = encode_column_metadata(&expected).unwrap();
        let (decoded, consumed) = decode_column_metadata(&encoded).unwrap();

        assert_eq!(decoded, expected);
        assert_eq!(consumed, encoded.len());
    }

    #[test]
    fn column_metadata_rejects_unknown_codec() {
        let encoded = encode_column_metadata(&metadata()).unwrap();
        let codec_offset = 2 + "active".len() + 2;
        let mut corrupted = encoded;
        corrupted[codec_offset] = 0xff;
        let err = decode_column_metadata(&corrupted).unwrap_err();

        assert!(matches!(err, CompactError::Unsupported("cmp3 codec id")));
    }

    #[test]
    fn column_metadata_rejects_unknown_value_type() {
        let encoded = encode_column_metadata(&metadata()).unwrap();
        let value_type_offset = 2 + "active".len();
        let mut corrupted = encoded;
        corrupted[value_type_offset] = 0xff;
        let err = decode_column_metadata(&corrupted).unwrap_err();

        assert!(matches!(
            err,
            CompactError::Unsupported("cmp3 value type id")
        ));
    }

    #[test]
    fn column_metadata_rejects_invalid_nullable_flag() {
        let encoded = encode_column_metadata(&metadata()).unwrap();
        let nullable_offset = 2 + "active".len() + 1;
        let mut corrupted = encoded;
        corrupted[nullable_offset] = 2;
        let err = decode_column_metadata(&corrupted).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("invalid cmp3 nullable flag")
        ));
    }

    #[test]
    fn column_metadata_rejects_invalid_null_count() {
        let mut invalid = metadata();
        invalid.null_count = invalid.value_count + 1;
        let err = encode_column_metadata(&invalid).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("cmp3 null count exceeds value count")
        ));
    }

    #[test]
    fn column_metadata_rejects_nulls_in_required_column() {
        let mut invalid = metadata();
        invalid.nullable = false;
        let err = encode_column_metadata(&invalid).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("cmp3 required column has null values")
        ));
    }

    #[test]
    fn column_metadata_rejects_auto_as_selected_codec() {
        let mut invalid = metadata();
        invalid.codec = SchemaCodec::Auto;
        let err = encode_column_metadata(&invalid).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("cmp3 selected codec does not match value type")
        ));
    }

    #[test]
    fn column_metadata_rejects_type_codec_mismatch() {
        let mut invalid = metadata();
        invalid.codec = SchemaCodec::Prefix;
        let err = encode_column_metadata(&invalid).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("cmp3 selected codec does not match value type")
        ));
    }

    #[test]
    fn column_metadata_rejects_truncated_codec_metadata() {
        let mut encoded = encode_column_metadata(&metadata()).unwrap();
        encoded.pop();
        let err = decode_column_metadata(&encoded).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("cmp3 codec metadata is truncated")
        ));
    }
}
