use serde::Deserialize;

use crate::{Codec, CompactError, Result, Transform, ValueType};

/// Schema for the current JSONL MVP.
///
/// The full roadmap needs multi-column blocks, nullability, strings, and nested
/// values. This first version is intentionally narrower: one required `u64`
/// column encoded with the `delta_varint_u64` pipeline. Keeping the contract
/// explicit prevents the decoder from guessing how to interpret JSON values.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Schema {
    pub columns: Vec<ColumnSchema>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ColumnSchema {
    pub name: String,
    #[serde(rename = "type")]
    pub value_type: SchemaValueType,
    pub codec: SchemaCodec,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SchemaValueType {
    U64,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SchemaCodec {
    DeltaVarintU64,
}

impl Schema {
    pub fn from_yaml(input: &str) -> Result<Self> {
        serde_yaml::from_str(input).map_err(|_| CompactError::InvalidInput("invalid schema"))
    }

    /// Return the single supported column for the JSONL MVP.
    ///
    /// Later schema versions should move this to a richer planner that can
    /// build multiple column blocks. For now, rejecting anything else keeps the
    /// file format honest and prevents silent data loss.
    pub fn single_u64_column(&self) -> Result<&ColumnSchema> {
        let [column] = self.columns.as_slice() else {
            return Err(CompactError::Unsupported(
                "schema must contain exactly one column",
            ));
        };

        if column.value_type != SchemaValueType::U64 || column.codec != SchemaCodec::DeltaVarintU64
        {
            return Err(CompactError::Unsupported("schema column codec"));
        }

        Ok(column)
    }
}

impl ColumnSchema {
    pub fn encode_config(&self) -> crate::EncodeConfig {
        crate::EncodeConfig {
            value_type: ValueType::U64,
            transform: Transform::Delta,
            codec: Codec::DeltaVarintU64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Schema, SchemaCodec, SchemaValueType};
    use crate::CompactError;

    #[test]
    fn schema_parses_single_u64_delta_varint_column() {
        let schema = Schema::from_yaml(
            r#"
columns:
  - name: ts
    type: u64
    codec: delta_varint_u64
"#,
        )
        .unwrap();

        let column = schema.single_u64_column().unwrap();

        assert_eq!(column.name, "ts");
        assert_eq!(column.value_type, SchemaValueType::U64);
        assert_eq!(column.codec, SchemaCodec::DeltaVarintU64);
    }

    #[test]
    fn schema_rejects_invalid_yaml() {
        let err = Schema::from_yaml("columns: [").unwrap_err();

        assert!(matches!(err, CompactError::InvalidInput("invalid schema")));
    }

    #[test]
    fn schema_rejects_multiple_columns_for_mvp() {
        let schema = Schema::from_yaml(
            r#"
columns:
  - name: ts
    type: u64
    codec: delta_varint_u64
  - name: id
    type: u64
    codec: delta_varint_u64
"#,
        )
        .unwrap();
        let err = schema.single_u64_column().unwrap_err();

        assert!(matches!(
            err,
            CompactError::Unsupported("schema must contain exactly one column")
        ));
    }
}
