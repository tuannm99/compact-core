use serde::Deserialize;

use crate::{Codec, CompactError, Result, Transform, ValueType};

/// Declarative schema shared by the stable CMP2 path and the CMP3 planner.
///
/// Call [`Schema::supported_columns`] before using a schema with CMP2 and
/// [`Schema::supported_columns_v3`] before planning CMP3 columns. Keeping the
/// version-specific validation explicit prevents older encoders from silently
/// accepting nullable values or codecs they cannot serialize.
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
    #[serde(default)]
    pub nullable: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SchemaValueType {
    Bool,
    String,
    U64,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SchemaCodec {
    Auto,
    Bitmap,
    Bitpack,
    Dictionary,
    DeltaVarintU64,
    Prefix,
    Rle,
    Stored,
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
            match (column.value_type, column.codec, column.nullable) {
                (SchemaValueType::U64, SchemaCodec::DeltaVarintU64, false) => {}
                (SchemaValueType::String, SchemaCodec::Dictionary, false) => {}
                (SchemaValueType::String, SchemaCodec::Rle, false) => {}
                _ => return Err(CompactError::Unsupported("schema column codec")),
            }
        }

        Ok(&self.columns)
    }

    /// Validate schema combinations supported by the v0.3 column format.
    ///
    /// This method validates format intent only. Individual codecs become
    /// executable as their implementation phases land; keeping v0.3 validation
    /// separate prevents the stable CMP2 path from accepting unsupported
    /// nullable or adaptive schemas.
    pub fn supported_columns_v3(&self) -> Result<&[ColumnSchema]> {
        if self.columns.is_empty() {
            return Err(CompactError::Unsupported(
                "schema must contain at least one column",
            ));
        }

        for (index, column) in self.columns.iter().enumerate() {
            if column.name.is_empty() {
                return Err(CompactError::InvalidInput(
                    "schema column name must not be empty",
                ));
            }

            if self.columns[..index]
                .iter()
                .any(|previous| previous.name == column.name)
            {
                return Err(CompactError::InvalidInput(
                    "schema column names must be unique",
                ));
            }

            let supported = match column.value_type {
                SchemaValueType::U64 => matches!(
                    column.codec,
                    SchemaCodec::Auto
                        | SchemaCodec::Bitpack
                        | SchemaCodec::DeltaVarintU64
                        | SchemaCodec::Stored
                ),
                SchemaValueType::String => matches!(
                    column.codec,
                    SchemaCodec::Auto
                        | SchemaCodec::Dictionary
                        | SchemaCodec::Prefix
                        | SchemaCodec::Rle
                        | SchemaCodec::Stored
                ),
                SchemaValueType::Bool => matches!(
                    column.codec,
                    SchemaCodec::Auto | SchemaCodec::Bitmap | SchemaCodec::Stored
                ),
            };

            if !supported {
                return Err(CompactError::Unsupported("v0.3 schema column codec"));
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
            (SchemaValueType::String, SchemaCodec::Dictionary) => crate::EncodeConfig {
                value_type: ValueType::String,
                transform: Transform::None,
                codec: Codec::ColumnBlock,
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
    codec: dictionary
"#,
        )
        .unwrap();

        let columns = schema.supported_columns().unwrap();

        assert_eq!(columns.len(), 3);
        assert_eq!(columns[0].name, "ts");
        assert_eq!(columns[0].value_type, SchemaValueType::U64);
        assert_eq!(columns[0].codec, SchemaCodec::DeltaVarintU64);
        assert!(!columns[0].nullable);
        assert_eq!(columns[1].name, "user_id");
        assert_eq!(columns[2].name, "level");
        assert_eq!(columns[2].value_type, SchemaValueType::String);
        assert_eq!(columns[2].codec, SchemaCodec::Dictionary);
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

    #[test]
    fn v3_schema_parses_bool_nullable_and_auto_columns() {
        let schema = Schema::from_yaml(
            r#"
columns:
  - name: ts
    type: u64
    codec: auto
  - name: active
    type: bool
    codec: bitmap
    nullable: true
  - name: service
    type: string
    codec: prefix
    nullable: true
"#,
        )
        .unwrap();
        let columns = schema.supported_columns_v3().unwrap();

        assert_eq!(columns[0].codec, SchemaCodec::Auto);
        assert_eq!(columns[1].value_type, SchemaValueType::Bool);
        assert!(columns[1].nullable);
        assert_eq!(columns[2].codec, SchemaCodec::Prefix);
    }

    #[test]
    fn v2_schema_rejects_v3_nullable_columns() {
        let schema = Schema::from_yaml(
            r#"
columns:
  - name: ts
    type: u64
    codec: delta_varint_u64
    nullable: true
"#,
        )
        .unwrap();
        let err = schema.supported_columns().unwrap_err();

        assert!(matches!(
            err,
            CompactError::Unsupported("schema column codec")
        ));
    }

    #[test]
    fn v3_schema_rejects_type_codec_mismatch() {
        let schema = Schema::from_yaml(
            r#"
columns:
  - name: active
    type: bool
    codec: prefix
"#,
        )
        .unwrap();
        let err = schema.supported_columns_v3().unwrap_err();

        assert!(matches!(
            err,
            CompactError::Unsupported("v0.3 schema column codec")
        ));
    }

    #[test]
    fn v3_schema_rejects_duplicate_column_names() {
        let schema = Schema::from_yaml(
            r#"
columns:
  - name: ts
    type: u64
    codec: auto
  - name: ts
    type: u64
    codec: stored
"#,
        )
        .unwrap();
        let err = schema.supported_columns_v3().unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("schema column names must be unique")
        ));
    }
}
