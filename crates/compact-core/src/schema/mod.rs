use serde::Deserialize;

use crate::{Codec, CompactError, Result, Transform, ValueType};

/// Schema for the current JSONL MVP.
///
/// The full roadmap needs nullability, nested values, and adaptive codec
/// selection. This version supports required `u64` columns encoded with
/// `delta_varint_u64` and required string columns encoded with RLE. Keeping that
/// contract explicit prevents the decoder from guessing how to interpret JSON
/// values.
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
    String,
    U64,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SchemaCodec {
    DeltaVarintU64,
    Rle,
}

impl Schema {
    pub fn from_yaml(input: &str) -> Result<Self> {
        serde_yaml::from_str(input).map_err(|_| CompactError::InvalidInput("invalid schema"))
    }

    /// Validate and return all supported columns in schema order.
    ///
    /// Later schema versions should move this to a richer planner. For now,
    /// validation stays centralized so JSONL encode/decode cannot accidentally
    /// skip unsupported columns.
    pub fn supported_columns(&self) -> Result<&[ColumnSchema]> {
        if self.columns.is_empty() {
            return Err(CompactError::Unsupported(
                "schema must contain at least one column",
            ));
        }

        for column in &self.columns {
            match (column.value_type, column.codec) {
                (SchemaValueType::U64, SchemaCodec::DeltaVarintU64) => {}
                (SchemaValueType::String, SchemaCodec::Rle) => {}
                _ => return Err(CompactError::Unsupported("schema column codec")),
            }
        }

        Ok(&self.columns)
    }
}

impl ColumnSchema {
    pub fn encode_config(&self) -> crate::EncodeConfig {
        match (self.value_type, self.codec) {
            (SchemaValueType::U64, SchemaCodec::DeltaVarintU64) => crate::EncodeConfig {
                value_type: ValueType::U64,
                transform: Transform::Delta,
                codec: Codec::DeltaVarintU64,
            },
            (SchemaValueType::String, SchemaCodec::Rle) => crate::EncodeConfig {
                value_type: ValueType::String,
                transform: Transform::None,
                codec: Codec::Rle,
            },
            _ => crate::EncodeConfig {
                value_type: ValueType::RawBytes,
                transform: Transform::None,
                codec: Codec::Huffman,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Schema, SchemaCodec, SchemaValueType};
    use crate::CompactError;

    #[test]
    fn schema_parses_u64_delta_varint_columns() {
        let schema = Schema::from_yaml(
            r#"
columns:
  - name: ts
    type: u64
    codec: delta_varint_u64
  - name: user_id
    type: u64
    codec: delta_varint_u64
  - name: level
    type: string
    codec: rle
"#,
        )
        .unwrap();

        let columns = schema.supported_columns().unwrap();

        assert_eq!(columns.len(), 3);
        assert_eq!(columns[0].name, "ts");
        assert_eq!(columns[0].value_type, SchemaValueType::U64);
        assert_eq!(columns[0].codec, SchemaCodec::DeltaVarintU64);
        assert_eq!(columns[1].name, "user_id");
        assert_eq!(columns[2].name, "level");
        assert_eq!(columns[2].value_type, SchemaValueType::String);
        assert_eq!(columns[2].codec, SchemaCodec::Rle);
    }

    #[test]
    fn schema_rejects_invalid_yaml() {
        let err = Schema::from_yaml("columns: [").unwrap_err();

        assert!(matches!(err, CompactError::InvalidInput("invalid schema")));
    }

    #[test]
    fn schema_rejects_empty_columns() {
        let schema = Schema::from_yaml(
            r#"
columns: []
"#,
        )
        .unwrap();
        let err = schema.supported_columns().unwrap_err();

        assert!(matches!(
            err,
            CompactError::Unsupported("schema must contain at least one column")
        ));
    }
}
