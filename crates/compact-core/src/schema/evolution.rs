//! Checked schema evolution for files whose physical schema is known.
//!
//! Existing CMP2-CMP4 files identify columns by physical name. This module
//! keeps a stable numeric identity in an external schema revision, allowing a
//! new reader to rename, add, or remove logical columns without changing old
//! file bytes. Embedding revision metadata is intentionally deferred until a
//! versioned migration format exists.

use std::collections::{HashMap, HashSet};
use std::io::Cursor;

use serde::Deserialize;
use serde_json::{Map, Value};

use super::{ColumnSchema, Schema, SchemaCodec, SchemaValueType};
use crate::storage::{StorageFormat, detect};
use crate::{CompactError, Result};

/// One externally managed schema revision.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct SchemaRevision {
    /// Monotonically increasing revision chosen by the schema owner.
    pub revision: u64,
    pub columns: Vec<EvolvedColumn>,
}

/// Logical column metadata used to match columns across schema revisions.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct EvolvedColumn {
    /// Stable identity. It must never be reused for another logical column.
    pub stable_id: u32,
    pub name: String,
    #[serde(rename = "type")]
    pub value_type: SchemaValueType,
    pub codec: SchemaCodec,
    #[serde(default)]
    pub nullable: bool,
    /// Historical names are documentation and migration lookup hints. Stable
    /// identity, not an alias string, is authoritative during comparison.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Value synthesized when this column did not exist in the writer schema.
    #[serde(default)]
    pub default: Option<Value>,
}

/// A concrete operation required to present writer rows as reader rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvolutionAction {
    Read {
        stable_id: u32,
        source_name: String,
        target_name: String,
    },
    FillDefault {
        stable_id: u32,
        target_name: String,
        value: Value,
    },
    FillNull {
        stable_id: u32,
        target_name: String,
    },
    Drop {
        stable_id: u32,
        source_name: String,
    },
}

/// A precise reason that a writer revision cannot be read as a reader revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvolutionIssue {
    ReaderRevisionIsOlder {
        writer_revision: u64,
        reader_revision: u64,
    },
    ValueTypeChanged {
        stable_id: u32,
        writer_type: SchemaValueType,
        reader_type: SchemaValueType,
    },
    NullabilityTightened {
        stable_id: u32,
        column: String,
    },
    RequiredColumnAddedWithoutDefault {
        stable_id: u32,
        column: String,
    },
}

/// Compatibility decision plus the exact operations needed for conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvolutionAssessment {
    pub writer_revision: u64,
    pub reader_revision: u64,
    pub actions: Vec<EvolutionAction>,
    pub issues: Vec<EvolutionIssue>,
}

impl EvolutionAssessment {
    pub fn is_compatible(&self) -> bool {
        self.issues.is_empty()
    }
}

impl SchemaRevision {
    /// Parse and validate an evolution schema from YAML.
    pub fn from_yaml(input: &str) -> Result<Self> {
        let revision: Self = serde_yaml::from_str(input)
            .map_err(|_| CompactError::InvalidInput("invalid schema revision"))?;
        revision.validate()?;
        Ok(revision)
    }

    /// Validate stable identities, names, defaults, and physical codecs.
    pub fn validate(&self) -> Result<()> {
        if self.revision == 0 {
            return Err(CompactError::InvalidInput(
                "schema revision must be positive",
            ));
        }
        if self.columns.is_empty() {
            return Err(CompactError::InvalidInput(
                "schema revision must contain columns",
            ));
        }

        let mut stable_ids = HashSet::new();
        let mut names = HashSet::new();
        for column in &self.columns {
            if column.stable_id == 0 {
                return Err(CompactError::InvalidInput(
                    "stable column id must be positive",
                ));
            }
            if !stable_ids.insert(column.stable_id) {
                return Err(CompactError::InvalidInput(
                    "stable column ids must be unique",
                ));
            }
            validate_name(&column.name)?;
            if !names.insert(column.name.as_str()) {
                return Err(CompactError::InvalidInput(
                    "schema revision names and aliases must be unique",
                ));
            }
            for alias in &column.aliases {
                validate_name(alias)?;
                if !names.insert(alias.as_str()) {
                    return Err(CompactError::InvalidInput(
                        "schema revision names and aliases must be unique",
                    ));
                }
            }
            validate_default(column)?;
        }

        self.physical_schema().supported_columns_v3()?;
        Ok(())
    }

    /// Build the physical schema required by the version-specific decoder.
    pub fn physical_schema(&self) -> Schema {
        Schema {
            columns: self
                .columns
                .iter()
                .map(|column| ColumnSchema {
                    name: column.name.clone(),
                    value_type: column.value_type,
                    codec: column.codec,
                    nullable: column.nullable,
                })
                .collect(),
        }
    }
}

/// Compare the schema that wrote a file with the schema requested by a reader.
///
/// Codec changes are compatible because the writer schema still selects the
/// physical decoder. Type changes and nullable-to-required changes are unsafe.
pub fn assess(writer: &SchemaRevision, reader: &SchemaRevision) -> Result<EvolutionAssessment> {
    writer.validate()?;
    reader.validate()?;

    let writer_by_id = writer
        .columns
        .iter()
        .map(|column| (column.stable_id, column))
        .collect::<HashMap<_, _>>();
    let reader_ids = reader
        .columns
        .iter()
        .map(|column| column.stable_id)
        .collect::<HashSet<_>>();
    let mut actions = Vec::new();
    let mut issues = Vec::new();

    if reader.revision < writer.revision {
        issues.push(EvolutionIssue::ReaderRevisionIsOlder {
            writer_revision: writer.revision,
            reader_revision: reader.revision,
        });
    }

    for reader_column in &reader.columns {
        match writer_by_id.get(&reader_column.stable_id) {
            Some(writer_column) => {
                if writer_column.value_type != reader_column.value_type {
                    issues.push(EvolutionIssue::ValueTypeChanged {
                        stable_id: reader_column.stable_id,
                        writer_type: writer_column.value_type,
                        reader_type: reader_column.value_type,
                    });
                    continue;
                }
                if writer_column.nullable && !reader_column.nullable {
                    issues.push(EvolutionIssue::NullabilityTightened {
                        stable_id: reader_column.stable_id,
                        column: reader_column.name.clone(),
                    });
                    continue;
                }
                actions.push(EvolutionAction::Read {
                    stable_id: reader_column.stable_id,
                    source_name: writer_column.name.clone(),
                    target_name: reader_column.name.clone(),
                });
            }
            None => {
                if let Some(value) = &reader_column.default {
                    actions.push(EvolutionAction::FillDefault {
                        stable_id: reader_column.stable_id,
                        target_name: reader_column.name.clone(),
                        value: value.clone(),
                    });
                } else if reader_column.nullable {
                    actions.push(EvolutionAction::FillNull {
                        stable_id: reader_column.stable_id,
                        target_name: reader_column.name.clone(),
                    });
                } else {
                    issues.push(EvolutionIssue::RequiredColumnAddedWithoutDefault {
                        stable_id: reader_column.stable_id,
                        column: reader_column.name.clone(),
                    });
                }
            }
        }
    }

    for writer_column in &writer.columns {
        if !reader_ids.contains(&writer_column.stable_id) {
            actions.push(EvolutionAction::Drop {
                stable_id: writer_column.stable_id,
                source_name: writer_column.name.clone(),
            });
        }
    }

    Ok(EvolutionAssessment {
        writer_revision: writer.revision,
        reader_revision: reader.revision,
        actions,
        issues,
    })
}

/// Decode CMP2-CMP4 using the physical writer schema, then project each row
/// through the checked reader evolution plan.
pub fn decode_jsonl(
    data: &[u8],
    writer: &SchemaRevision,
    reader: &SchemaRevision,
) -> Result<String> {
    let assessment = assess(writer, reader)?;
    if !assessment.is_compatible() {
        return Err(CompactError::InvalidInput(
            "schema revisions are incompatible",
        ));
    }

    let writer_schema = writer.physical_schema();
    let physical_jsonl = match detect(data)? {
        StorageFormat::V1 => {
            return Err(CompactError::Unsupported(
                "schema evolution requires a columnar format",
            ));
        }
        StorageFormat::V2 => {
            let bytes = crate::streaming::decode_jsonl_stream(
                Cursor::new(data),
                Vec::new(),
                writer_schema,
            )?;
            String::from_utf8(bytes)
                .map_err(|_| CompactError::InvalidInput("decoded jsonl must be utf-8"))?
        }
        StorageFormat::V3 => crate::io::v3::decode_jsonl(data, &writer_schema)?,
        StorageFormat::V4 => crate::io::v4::decode_jsonl(data, &writer_schema)?,
    };

    apply_actions(&physical_jsonl, &assessment.actions)
}

fn apply_actions(input: &str, actions: &[EvolutionAction]) -> Result<String> {
    let mut output = String::new();

    for line in input.lines().filter(|line| !line.trim().is_empty()) {
        let value: Value = serde_json::from_str(line)
            .map_err(|_| CompactError::InvalidInput("decoded jsonl row is invalid"))?;
        let source = value.as_object().ok_or(CompactError::InvalidInput(
            "decoded jsonl row must be an object",
        ))?;
        let mut target = Map::new();

        for action in actions {
            match action {
                EvolutionAction::Read {
                    source_name,
                    target_name,
                    ..
                } => {
                    let value = source.get(source_name).ok_or(CompactError::InvalidInput(
                        "writer schema column is missing from decoded row",
                    ))?;
                    target.insert(target_name.clone(), value.clone());
                }
                EvolutionAction::FillDefault {
                    target_name, value, ..
                } => {
                    target.insert(target_name.clone(), value.clone());
                }
                EvolutionAction::FillNull { target_name, .. } => {
                    target.insert(target_name.clone(), Value::Null);
                }
                EvolutionAction::Drop { .. } => {}
            }
        }

        output.push_str(
            &serde_json::to_string(&target)
                .map_err(|_| CompactError::InvalidInput("failed to render evolved row"))?,
        );
        output.push('\n');
    }

    Ok(output)
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(CompactError::InvalidInput(
            "schema revision name must not be empty",
        ));
    }
    Ok(())
}

fn validate_default(column: &EvolvedColumn) -> Result<()> {
    let Some(value) = &column.default else {
        return Ok(());
    };
    if value.is_null() {
        if column.nullable {
            return Ok(());
        }
        return Err(CompactError::InvalidInput(
            "required column default must not be null",
        ));
    }

    let valid = match column.value_type {
        SchemaValueType::Bool => value.is_boolean(),
        SchemaValueType::String => value.is_string(),
        SchemaValueType::U64 => value.as_u64().is_some(),
    };
    if !valid {
        return Err(CompactError::InvalidInput(
            "column default does not match value type",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use serde_json::Value;

    use super::{EvolutionAction, EvolutionIssue, SchemaRevision, assess, decode_jsonl};
    use crate::io::v4::EncodeOptions;
    use crate::streaming::BlockOptions;

    fn revision_one() -> SchemaRevision {
        SchemaRevision::from_yaml(
            r#"
revision: 1
columns:
  - stable_id: 1
    name: id
    type: u64
    codec: delta_bitpack
  - stable_id: 2
    name: service
    type: string
    codec: prefix
    nullable: true
"#,
        )
        .unwrap()
    }

    #[test]
    fn compatible_plan_handles_rename_add_default_and_drop() {
        let reader = SchemaRevision::from_yaml(
            r#"
revision: 2
columns:
  - stable_id: 1
    name: event_id
    aliases: [id]
    type: u64
    codec: bitpack
  - stable_id: 3
    name: active
    type: bool
    codec: bitmap
    default: true
"#,
        )
        .unwrap();

        let assessment = assess(&revision_one(), &reader).unwrap();

        assert!(assessment.is_compatible());
        assert!(matches!(
            &assessment.actions[0],
            EvolutionAction::Read {
                source_name,
                target_name,
                ..
            } if source_name == "id" && target_name == "event_id"
        ));
        assert!(matches!(
            &assessment.actions[1],
            EvolutionAction::FillDefault { target_name, .. } if target_name == "active"
        ));
        assert!(matches!(
            &assessment.actions[2],
            EvolutionAction::Drop { source_name, .. } if source_name == "service"
        ));
    }

    #[test]
    fn incompatible_plan_reports_each_unsafe_change() {
        let reader = SchemaRevision::from_yaml(
            r#"
revision: 2
columns:
  - stable_id: 1
    name: id
    type: string
    codec: prefix
  - stable_id: 2
    name: service
    type: string
    codec: prefix
  - stable_id: 3
    name: required_new
    type: bool
    codec: bitmap
"#,
        )
        .unwrap();

        let assessment = assess(&revision_one(), &reader).unwrap();

        assert!(!assessment.is_compatible());
        assert!(matches!(
            assessment.issues[0],
            EvolutionIssue::ValueTypeChanged { stable_id: 1, .. }
        ));
        assert!(matches!(
            assessment.issues[1],
            EvolutionIssue::NullabilityTightened { stable_id: 2, .. }
        ));
        assert!(matches!(
            assessment.issues[2],
            EvolutionIssue::RequiredColumnAddedWithoutDefault { stable_id: 3, .. }
        ));
    }

    #[test]
    fn revision_validation_rejects_duplicate_identity_and_bad_defaults() {
        let duplicate = SchemaRevision::from_yaml(
            "revision: 1\ncolumns:\n  - {stable_id: 1, name: a, type: u64, codec: stored}\n  - {stable_id: 1, name: b, type: u64, codec: stored}\n",
        )
        .unwrap_err();
        let bad_default = SchemaRevision::from_yaml(
            "revision: 1\ncolumns:\n  - {stable_id: 1, name: a, type: u64, codec: stored, default: nope}\n",
        )
        .unwrap_err();

        assert!(
            duplicate
                .to_string()
                .contains("stable column ids must be unique")
        );
        assert!(
            bad_default
                .to_string()
                .contains("column default does not match value type")
        );
    }

    #[test]
    fn evolved_cmp4_decode_renames_drops_and_fills_columns() {
        let writer = revision_one();
        let reader = SchemaRevision::from_yaml(
            r#"
revision: 2
columns:
  - stable_id: 1
    name: event_id
    aliases: [id]
    type: u64
    codec: bitpack
  - stable_id: 3
    name: active
    type: bool
    codec: bitmap
    default: true
  - stable_id: 4
    name: region
    type: string
    codec: stored
    nullable: true
"#,
        )
        .unwrap();
        let input = "{\"id\":1,\"service\":\"api\"}\n{\"id\":2,\"service\":null}\n";
        let encoded =
            crate::io::v4::encode_jsonl(input, &writer.physical_schema(), EncodeOptions::default())
                .unwrap();

        let decoded = decode_jsonl(&encoded, &writer, &reader).unwrap();
        let rows = decoded
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();

        assert_eq!(
            rows[0],
            serde_json::json!({"event_id": 1, "active": true, "region": null})
        );
        assert_eq!(
            rows[1],
            serde_json::json!({"event_id": 2, "active": true, "region": null})
        );
    }

    #[test]
    fn compatible_revision_decodes_legacy_cmp2_cmp3_and_cmp4_files() {
        let writer = SchemaRevision::from_yaml(
            "revision: 1\ncolumns:\n  - {stable_id: 1, name: id, type: u64, codec: delta_varint_u64}\n",
        )
        .unwrap();
        let reader = SchemaRevision::from_yaml(
            "revision: 2\ncolumns:\n  - {stable_id: 1, name: event_id, type: u64, codec: bitpack, aliases: [id]}\n  - {stable_id: 2, name: active, type: bool, codec: bitmap, default: true}\n",
        )
        .unwrap();
        let input = "{\"id\":1}\n{\"id\":2}\n";
        let physical = writer.physical_schema();
        let files = [
            crate::streaming::encode_jsonl_stream(
                Cursor::new(input),
                Vec::new(),
                physical.clone(),
                BlockOptions::default(),
            )
            .unwrap(),
            crate::io::v3::encode_jsonl(input, &physical).unwrap(),
            crate::io::v4::encode_jsonl(input, &physical, EncodeOptions::default()).unwrap(),
        ];

        for file in files {
            let decoded = decode_jsonl(&file, &writer, &reader).unwrap();
            let rows = decoded
                .lines()
                .map(|line| serde_json::from_str::<Value>(line).unwrap())
                .collect::<Vec<_>>();

            assert_eq!(
                rows,
                vec![
                    serde_json::json!({"event_id": 1, "active": true}),
                    serde_json::json!({"event_id": 2, "active": true}),
                ]
            );
        }
    }

    #[test]
    fn checked_in_compatibility_fixture_matrix_has_stable_decisions() {
        let writer =
            SchemaRevision::from_yaml(include_str!("../../../../testdata/v0.9/writer-v1.yml"))
                .unwrap();
        let compatible = SchemaRevision::from_yaml(include_str!(
            "../../../../testdata/v0.9/reader-compatible-v2.yml"
        ))
        .unwrap();
        let incompatible = SchemaRevision::from_yaml(include_str!(
            "../../../../testdata/v0.9/reader-incompatible-v2.yml"
        ))
        .unwrap();

        assert!(assess(&writer, &compatible).unwrap().is_compatible());
        let rejected = assess(&writer, &incompatible).unwrap();
        assert!(!rejected.is_compatible());
        assert_eq!(rejected.issues.len(), 2);
    }
}
