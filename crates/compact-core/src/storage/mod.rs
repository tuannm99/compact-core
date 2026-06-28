//! Production storage compatibility and validation APIs.
//!
//! This module is the format-independent entry point for tools that must inspect
//! a file before choosing a version-specific reader. Detection only trusts the
//! fixed magic and version bytes. Validation then delegates to the owning
//! decoder so checksum and structural rules remain defined in one place.

use std::fmt;
use std::io::Cursor;

use crate::{
    CompactError, MAGIC_V1, MAGIC_V2, MAGIC_V3, MAGIC_V4, Result, VERSION_V1, VERSION_V2,
    VERSION_V3, VERSION_V4, framing,
};

pub mod migration;
pub mod repair;

/// A storage format understood by this release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StorageFormat {
    V1,
    V2,
    V3,
    V4,
}

impl StorageFormat {
    /// Return the version byte persisted after the four-byte file magic.
    pub const fn version(self) -> u8 {
        match self {
            Self::V1 => VERSION_V1,
            Self::V2 => VERSION_V2,
            Self::V3 => VERSION_V3,
            Self::V4 => VERSION_V4,
        }
    }
}

impl fmt::Display for StorageFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "CMP{}", self.version())
    }
}

/// Inclusive reader-version range used during compatibility negotiation.
///
/// A service can use this policy to reject a valid file before expensive
/// decoding when the file is outside the versions deployed in that service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompatibilityPolicy {
    pub minimum: StorageFormat,
    pub maximum: StorageFormat,
}

impl CompatibilityPolicy {
    /// Construct a checked inclusive range.
    pub fn new(minimum: StorageFormat, maximum: StorageFormat) -> Result<Self> {
        if minimum > maximum {
            return Err(CompactError::InvalidInput(
                "minimum storage version exceeds maximum",
            ));
        }

        Ok(Self { minimum, maximum })
    }

    /// Return whether this reader policy accepts `format`.
    pub fn supports(self, format: StorageFormat) -> bool {
        (self.minimum..=self.maximum).contains(&format)
    }
}

impl Default for CompatibilityPolicy {
    fn default() -> Self {
        Self {
            minimum: StorageFormat::V1,
            maximum: StorageFormat::V4,
        }
    }
}

/// Summary returned after a complete schema-independent structural validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidationReport {
    pub format: StorageFormat,
    pub file_size: u64,
    /// Frames, blocks, or row groups validated, depending on the format.
    pub storage_units: u64,
    /// Logical rows when the format stores this information without a schema.
    pub total_rows: Option<u64>,
    /// Whether an optional terminal index is present for formats that support
    /// both sealed files and valid append streams.
    pub has_footer_index: Option<bool>,
}

/// Detect a known storage format and verify that its persisted version matches
/// its magic. A known magic with a different version is not silently accepted.
pub fn detect(data: &[u8]) -> Result<StorageFormat> {
    let magic = data
        .get(..4)
        .ok_or(CompactError::InvalidInput("storage header is truncated"))?;
    let version = *data
        .get(4)
        .ok_or(CompactError::InvalidInput("storage version is truncated"))?;

    let format = match magic {
        bytes if bytes == MAGIC_V1 => StorageFormat::V1,
        bytes if bytes == MAGIC_V2 => StorageFormat::V2,
        bytes if bytes == MAGIC_V3 => StorageFormat::V3,
        bytes if bytes == MAGIC_V4 => StorageFormat::V4,
        _ => return Err(CompactError::InvalidInput("unknown storage magic")),
    };

    if version != format.version() {
        return Err(CompactError::Unsupported(
            "storage magic/version combination",
        ));
    }

    Ok(format)
}

/// Detect a file and reject it when the reader's declared range cannot handle
/// that format.
pub fn negotiate(data: &[u8], policy: CompatibilityPolicy) -> Result<StorageFormat> {
    let format = detect(data)?;
    if !policy.supports(format) {
        return Err(CompactError::Unsupported(
            "storage version is outside reader compatibility range",
        ));
    }

    Ok(format)
}

/// Validate all schema-independent structure and checksums in a supported file.
///
/// This does not replace schema-aware decode tests. In particular, it cannot
/// prove that decoded column values match an external schema.
pub fn validate(data: &[u8]) -> Result<ValidationReport> {
    let format = negotiate(data, CompatibilityPolicy::default())?;
    let file_size = u64::try_from(data.len())
        .map_err(|_| CompactError::InvalidInput("storage file is too large"))?;

    let (storage_units, total_rows, has_footer_index) = match format {
        StorageFormat::V1 => {
            framing::decode_v1(data)?;
            (1, None, None)
        }
        StorageFormat::V2 => {
            let inspect = crate::streaming::inspect_stream(Cursor::new(data))?;
            (
                u64::try_from(inspect.blocks.len())
                    .map_err(|_| CompactError::InvalidInput("block count is too large"))?,
                Some(inspect.total_rows),
                Some(inspect.footer_index.is_some()),
            )
        }
        StorageFormat::V3 => {
            let inspect = crate::io::v3::inspect_jsonl(data)?;
            (1, Some(inspect.row_count), None)
        }
        StorageFormat::V4 => {
            let footer = crate::io::v4::validate_file(data)?;
            (
                u64::try_from(footer.row_groups.len())
                    .map_err(|_| CompactError::InvalidInput("row group count is too large"))?,
                Some(footer.total_row_count),
                Some(true),
            )
        }
    };

    Ok(ValidationReport {
        format,
        file_size,
        storage_units,
        total_rows,
        has_footer_index,
    })
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{CompatibilityPolicy, StorageFormat, detect, negotiate, validate};
    use crate::schema::Schema;
    use crate::streaming::BlockOptions;
    use crate::{Codec, CompactError, framing};

    fn schema() -> Schema {
        Schema::from_yaml("columns:\n  - name: id\n    type: u64\n    codec: delta_varint_u64\n")
            .unwrap()
    }

    fn jsonl() -> &'static str {
        "{\"id\":1}\n{\"id\":2}\n"
    }

    #[test]
    fn detects_every_supported_format() {
        let v1 = framing::encode_v1(Codec::Rle, &[]);
        let v2 = crate::streaming::encode_jsonl_stream(
            Cursor::new(jsonl()),
            Vec::new(),
            schema(),
            BlockOptions {
                max_rows_per_block: 1,
                ..BlockOptions::default()
            },
        )
        .unwrap();
        let v3 = crate::io::v3::encode_jsonl(jsonl(), &schema()).unwrap();
        let v4 = crate::io::v4::encode_jsonl(
            jsonl(),
            &schema(),
            crate::io::v4::EncodeOptions { row_group_rows: 1 },
        )
        .unwrap();

        assert_eq!(detect(&v1).unwrap(), StorageFormat::V1);
        assert_eq!(detect(&v2).unwrap(), StorageFormat::V2);
        assert_eq!(detect(&v3).unwrap(), StorageFormat::V3);
        assert_eq!(detect(&v4).unwrap(), StorageFormat::V4);
    }

    #[test]
    fn negotiation_enforces_the_reader_range() {
        let file = crate::io::v4::encode_jsonl(
            jsonl(),
            &schema(),
            crate::io::v4::EncodeOptions::default(),
        )
        .unwrap();
        let policy = CompatibilityPolicy::new(StorageFormat::V2, StorageFormat::V3).unwrap();

        let error = negotiate(&file, policy).unwrap_err();

        assert!(matches!(
            error,
            CompactError::Unsupported("storage version is outside reader compatibility range")
        ));
    }

    #[test]
    fn rejects_inverted_compatibility_range() {
        let error = CompatibilityPolicy::new(StorageFormat::V4, StorageFormat::V2).unwrap_err();

        assert!(matches!(
            error,
            CompactError::InvalidInput("minimum storage version exceeds maximum")
        ));
    }

    #[test]
    fn rejects_known_magic_with_mismatched_version() {
        let mut file = framing::encode_v1(Codec::Rle, &[]);
        file[4] = 4;

        let error = detect(&file).unwrap_err();

        assert!(matches!(
            error,
            CompactError::Unsupported("storage magic/version combination")
        ));
    }

    #[test]
    fn validates_v2_metadata_and_checksums() {
        let mut file = crate::streaming::encode_jsonl_stream(
            Cursor::new(jsonl()),
            Vec::new(),
            schema(),
            BlockOptions {
                max_rows_per_block: 1,
                ..BlockOptions::default()
            },
        )
        .unwrap();
        let report = validate(&file).unwrap();

        assert_eq!(report.format, StorageFormat::V2);
        assert_eq!(report.storage_units, 2);
        assert_eq!(report.total_rows, Some(2));
        assert_eq!(report.has_footer_index, Some(true));

        file[30] ^= 0xff;
        assert!(validate(&file).is_err());
    }

    #[test]
    fn validates_every_cmp4_row_group_checksum() {
        let mut file = crate::io::v4::encode_jsonl(
            jsonl(),
            &schema(),
            crate::io::v4::EncodeOptions { row_group_rows: 1 },
        )
        .unwrap();
        let footer = crate::io::v4::inspect_footer(&file).unwrap();
        let payload_offset = footer.row_groups[1].columns[0].payload_offset as usize;
        let report = validate(&file).unwrap();

        assert_eq!(report.format, StorageFormat::V4);
        assert_eq!(report.storage_units, 2);
        assert_eq!(report.total_rows, Some(2));

        file[payload_offset] ^= 0xff;
        let error = validate(&file).unwrap_err();
        assert!(matches!(
            error,
            CompactError::InvalidInput("cmp4 row group checksum mismatch")
        ));
    }

    #[test]
    fn corruption_matrix_rejects_every_supported_header_truncation() {
        let files = [
            framing::encode_v1(Codec::Rle, &[]),
            crate::streaming::encode_jsonl_stream(
                Cursor::new(jsonl()),
                Vec::new(),
                schema(),
                BlockOptions::default(),
            )
            .unwrap(),
            crate::io::v3::encode_jsonl(jsonl(), &schema()).unwrap(),
            crate::io::v4::encode_jsonl(
                jsonl(),
                &schema(),
                crate::io::v4::EncodeOptions::default(),
            )
            .unwrap(),
        ];

        for file in files {
            for truncated_len in 0..5 {
                assert!(
                    validate(&file[..truncated_len]).is_err(),
                    "format header truncation at {truncated_len} bytes must fail"
                );
            }
        }
    }

    #[test]
    fn cmp4_corruption_matrix_recovers_only_groups_before_damage() {
        let file = crate::io::v4::encode_jsonl(
            jsonl(),
            &schema(),
            crate::io::v4::EncodeOptions { row_group_rows: 1 },
        )
        .unwrap();
        let footer = crate::io::v4::inspect_footer(&file).unwrap();

        for (position, row_group) in footer.row_groups.iter().enumerate() {
            let mut corrupted = file.clone();
            let offset = row_group.columns[0].payload_offset as usize;
            corrupted[offset] ^= 0xff;

            assert!(validate(&corrupted).is_err());
            let plan = crate::storage::repair::plan(&corrupted).unwrap();
            assert_eq!(plan.recovered_units, position as u64);
            let repaired = crate::storage::repair::execute(&corrupted, &plan).unwrap();
            assert!(validate(&repaired).is_ok());
        }
    }
}
