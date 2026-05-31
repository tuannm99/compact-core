//! JSONL conversion helpers for the first columnar MVP.
//!
//! This module is intentionally small. It bridges human-readable JSONL and the
//! already-tested numeric frame path, but it does not yet define the final
//! multi-column block format.

use serde_json::{Map, Value};

use crate::schema::Schema;
use crate::{CompactError, Result, decode_u64_frame, encode_u64_frame};

/// Encode JSONL rows using the current one-column `u64` schema.
///
/// Each non-empty line must be a JSON object containing the schema column as a
/// non-negative integer that fits in `u64`.
pub fn encode_jsonl(input: &str, schema: &Schema) -> Result<Vec<u8>> {
    let column = schema.single_u64_column()?;
    let mut values = Vec::new();

    for line in input.lines().filter(|line| !line.trim().is_empty()) {
        let value: Value =
            serde_json::from_str(line).map_err(|_| CompactError::InvalidInput("invalid jsonl"))?;
        let object = value
            .as_object()
            .ok_or(CompactError::InvalidInput("jsonl row must be an object"))?;
        let raw = object.get(&column.name).ok_or(CompactError::InvalidInput(
            "jsonl row missing schema column",
        ))?;
        let value = raw
            .as_u64()
            .ok_or(CompactError::InvalidInput("jsonl column must be u64"))?;

        values.push(value);
    }

    encode_u64_frame(&column.encode_config(), &values)
}

/// Decode a one-column `u64` frame back into JSONL.
///
/// Output uses compact JSON with one object per line and a trailing newline for
/// byte-stable CLI roundtrips.
pub fn decode_jsonl(frame: &[u8], schema: &Schema) -> Result<String> {
    let column = schema.single_u64_column()?;
    let values = decode_u64_frame(&column.encode_config(), frame)?;
    let mut out = String::new();

    for value in values {
        let mut object = Map::new();
        object.insert(column.name.clone(), Value::from(value));
        out.push_str(&Value::Object(object).to_string());
        out.push('\n');
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{decode_jsonl, encode_jsonl};
    use crate::CompactError;
    use crate::schema::Schema;

    fn schema() -> Schema {
        Schema::from_yaml(
            r#"
columns:
  - name: ts
    type: u64
    codec: delta_varint_u64
"#,
        )
        .unwrap()
    }

    #[test]
    fn jsonl_single_u64_column_roundtrip_is_byte_stable() {
        let input = "{\"ts\":100}\n{\"ts\":101}\n{\"ts\":130}\n";
        let encoded = encode_jsonl(input, &schema()).unwrap();
        let decoded = decode_jsonl(&encoded, &schema()).unwrap();

        assert_eq!(decoded, input);
    }

    #[test]
    fn jsonl_rejects_missing_column() {
        let err = encode_jsonl("{\"id\":100}\n", &schema()).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("jsonl row missing schema column")
        ));
    }

    #[test]
    fn jsonl_rejects_negative_u64_column() {
        let err = encode_jsonl("{\"ts\":-1}\n", &schema()).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("jsonl column must be u64")
        ));
    }

    #[test]
    fn jsonl_rejects_invalid_json() {
        let err = encode_jsonl("{bad json}\n", &schema()).unwrap_err();

        assert!(matches!(err, CompactError::InvalidInput("invalid jsonl")));
    }
}
