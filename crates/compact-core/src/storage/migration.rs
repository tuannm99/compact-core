//! Deterministic migration for external schema metadata documents.
//!
//! CMP2-CMP4 do not contain stable schema identities. v0.9 therefore migrates
//! the external YAML metadata used by schema evolution. Version 1 identifies
//! columns by name; version 2 adds explicit stable IDs and is directly readable
//! as [`crate::schema::evolution::SchemaRevision`].

use std::collections::{BTreeMap, HashSet};

use serde::Deserialize;
use serde_yaml::{Mapping, Value};

use crate::schema::ColumnSchema;
use crate::schema::evolution::SchemaRevision;
use crate::{CompactError, Result, checksum32};

const METADATA_VERSION_V1: u64 = 1;
const METADATA_VERSION_V2: u64 = 2;

/// Caller-supplied identities required for a safe name-based v1 migration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MigrationAssignments {
    pub stable_ids: BTreeMap<String, u32>,
}

/// Whether executing a migration plan changes output bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationAction {
    None,
    AddStableColumnIds,
}

/// Source-bound metadata migration decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationPlan {
    pub source_version: u64,
    pub target_version: u64,
    pub action: MigrationAction,
    pub source_len: u64,
    pub source_checksum: u32,
    pub column_count: u64,
    migrated: Vec<u8>,
}

#[derive(Debug, Deserialize)]
struct LegacyMetadata {
    metadata_version: u64,
    revision: u64,
    columns: Vec<ColumnSchema>,
}

/// Build a deterministic v1-to-v2 plan without changing the source.
///
/// Stable IDs must be supplied for every v1 column. Deriving IDs from position
/// or a name hash would silently change identity after reorder or rename.
pub fn plan(source: &[u8], assignments: &MigrationAssignments) -> Result<MigrationPlan> {
    let source_len = u64::try_from(source.len())
        .map_err(|_| CompactError::InvalidInput("metadata source is too large"))?;
    let source_checksum = checksum32(source);
    let text = std::str::from_utf8(source)
        .map_err(|_| CompactError::InvalidInput("metadata source must be utf-8"))?;
    let mut document: Value = serde_yaml::from_str(text)
        .map_err(|_| CompactError::InvalidInput("invalid metadata document"))?;
    let source_version = metadata_version(&document)?;

    match source_version {
        METADATA_VERSION_V1 => {
            let legacy: LegacyMetadata = serde_yaml::from_str(text)
                .map_err(|_| CompactError::InvalidInput("invalid v1 metadata document"))?;
            if legacy.metadata_version != METADATA_VERSION_V1 || legacy.revision == 0 {
                return Err(CompactError::InvalidInput("invalid v1 metadata revision"));
            }
            crate::schema::Schema {
                columns: legacy.columns.clone(),
            }
            .supported_columns_v3()?;
            validate_assignments(&legacy.columns, assignments)?;
            add_stable_ids(&mut document, assignments)?;
            set_metadata_version(&mut document, METADATA_VERSION_V2)?;
            let migrated = serde_yaml::to_string(&document)
                .map_err(|_| CompactError::InvalidInput("failed to encode migrated metadata"))?
                .into_bytes();
            validate_v2(&migrated)?;

            Ok(MigrationPlan {
                source_version,
                target_version: METADATA_VERSION_V2,
                action: MigrationAction::AddStableColumnIds,
                source_len,
                source_checksum,
                column_count: u64::try_from(legacy.columns.len()).map_err(|_| {
                    CompactError::InvalidInput("metadata column count is too large")
                })?,
                migrated,
            })
        }
        METADATA_VERSION_V2 => {
            let revision = validate_v2(source)?;
            if !assignments.stable_ids.is_empty() {
                return Err(CompactError::InvalidInput(
                    "v2 metadata migration does not accept column assignments",
                ));
            }
            Ok(MigrationPlan {
                source_version,
                target_version: METADATA_VERSION_V2,
                action: MigrationAction::None,
                source_len,
                source_checksum,
                column_count: u64::try_from(revision.columns.len()).map_err(|_| {
                    CompactError::InvalidInput("metadata column count is too large")
                })?,
                migrated: source.to_vec(),
            })
        }
        _ => Err(CompactError::Unsupported("metadata version")),
    }
}

/// Execute a reviewed plan and return a new metadata buffer.
pub fn execute(source: &[u8], plan: &MigrationPlan) -> Result<Vec<u8>> {
    let source_len = u64::try_from(source.len())
        .map_err(|_| CompactError::InvalidInput("metadata source is too large"))?;
    if source_len != plan.source_len || checksum32(source) != plan.source_checksum {
        return Err(CompactError::InvalidInput(
            "metadata source does not match migration plan",
        ));
    }

    validate_v2(&plan.migrated)?;
    Ok(plan.migrated.clone())
}

fn metadata_version(document: &Value) -> Result<u64> {
    root_mapping(document)?
        .get(Value::String("metadata_version".to_owned()))
        .and_then(Value::as_u64)
        .ok_or(CompactError::InvalidInput(
            "metadata_version must be a positive integer",
        ))
}

fn set_metadata_version(document: &mut Value, version: u64) -> Result<()> {
    root_mapping_mut(document)?.insert(
        Value::String("metadata_version".to_owned()),
        Value::Number(version.into()),
    );
    Ok(())
}

fn add_stable_ids(document: &mut Value, assignments: &MigrationAssignments) -> Result<()> {
    let columns = root_mapping_mut(document)?
        .get_mut(Value::String("columns".to_owned()))
        .and_then(Value::as_sequence_mut)
        .ok_or(CompactError::InvalidInput(
            "metadata columns must be a sequence",
        ))?;

    for column in columns {
        let mapping = column.as_mapping_mut().ok_or(CompactError::InvalidInput(
            "metadata column must be a mapping",
        ))?;
        let name = mapping
            .get(Value::String("name".to_owned()))
            .and_then(Value::as_str)
            .ok_or(CompactError::InvalidInput(
                "metadata column name must be a string",
            ))?;
        let stable_id = assignments
            .stable_ids
            .get(name)
            .ok_or(CompactError::InvalidInput(
                "missing stable id assignment for metadata column",
            ))?;
        mapping.insert(
            Value::String("stable_id".to_owned()),
            Value::Number(u64::from(*stable_id).into()),
        );
    }
    Ok(())
}

fn validate_assignments(
    columns: &[ColumnSchema],
    assignments: &MigrationAssignments,
) -> Result<()> {
    if assignments.stable_ids.len() != columns.len() {
        return Err(CompactError::InvalidInput(
            "stable id assignments must match metadata columns exactly",
        ));
    }
    let names = columns
        .iter()
        .map(|column| column.name.as_str())
        .collect::<HashSet<_>>();
    let mut ids = HashSet::new();
    for (name, stable_id) in &assignments.stable_ids {
        if !names.contains(name.as_str()) {
            return Err(CompactError::InvalidInput(
                "stable id assignment references unknown column",
            ));
        }
        if *stable_id == 0 {
            return Err(CompactError::InvalidInput(
                "stable column id must be positive",
            ));
        }
        if !ids.insert(*stable_id) {
            return Err(CompactError::InvalidInput(
                "stable column ids must be unique",
            ));
        }
    }
    Ok(())
}

fn validate_v2(source: &[u8]) -> Result<SchemaRevision> {
    let text = std::str::from_utf8(source)
        .map_err(|_| CompactError::InvalidInput("metadata source must be utf-8"))?;
    SchemaRevision::from_yaml(text)
}

fn root_mapping(document: &Value) -> Result<&Mapping> {
    document.as_mapping().ok_or(CompactError::InvalidInput(
        "metadata root must be a mapping",
    ))
}

fn root_mapping_mut(document: &mut Value) -> Result<&mut Mapping> {
    document.as_mapping_mut().ok_or(CompactError::InvalidInput(
        "metadata root must be a mapping",
    ))
}

#[cfg(test)]
mod tests {
    use super::{MigrationAction, MigrationAssignments, execute, plan};

    fn source() -> &'static [u8] {
        br#"metadata_version: 1
revision: 7
owner: storage-team
columns:
  - name: id
    type: u64
    codec: delta_bitpack
    logical_role: primary_key
  - name: service
    type: string
    codec: prefix
    nullable: true
"#
    }

    fn assignments() -> MigrationAssignments {
        MigrationAssignments {
            stable_ids: [("id".to_owned(), 10), ("service".to_owned(), 20)].into(),
        }
    }

    #[test]
    fn migrates_v1_with_explicit_ids_and_preserves_unknown_fields() {
        let migration = plan(source(), &assignments()).unwrap();
        let output = execute(source(), &migration).unwrap();
        let text = String::from_utf8(output).unwrap();

        assert_eq!(migration.action, MigrationAction::AddStableColumnIds);
        assert!(text.contains("metadata_version: 2"));
        assert!(text.contains("stable_id: 10"));
        assert!(text.contains("stable_id: 20"));
        assert!(text.contains("owner: storage-team"));
        assert!(text.contains("logical_role: primary_key"));
    }

    #[test]
    fn migrated_v2_document_is_byte_idempotent() {
        let first_plan = plan(source(), &assignments()).unwrap();
        let first = execute(source(), &first_plan).unwrap();
        let second_plan = plan(&first, &MigrationAssignments::default()).unwrap();
        let second = execute(&first, &second_plan).unwrap();

        assert_eq!(second_plan.action, MigrationAction::None);
        assert_eq!(second, first);
    }

    #[test]
    fn rejects_missing_duplicate_and_unknown_assignments() {
        let missing = MigrationAssignments {
            stable_ids: [("id".to_owned(), 1)].into(),
        };
        let duplicate = MigrationAssignments {
            stable_ids: [("id".to_owned(), 1), ("service".to_owned(), 1)].into(),
        };
        let unknown = MigrationAssignments {
            stable_ids: [("id".to_owned(), 1), ("other".to_owned(), 2)].into(),
        };

        assert!(plan(source(), &missing).is_err());
        assert!(plan(source(), &duplicate).is_err());
        assert!(plan(source(), &unknown).is_err());
    }

    #[test]
    fn stale_plan_rejects_changed_metadata_source() {
        let migration = plan(source(), &assignments()).unwrap();
        let mut changed = source().to_vec();
        changed.push(b'\n');

        assert!(execute(&changed, &migration).is_err());
    }
}
