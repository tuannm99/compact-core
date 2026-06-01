use std::ffi::{CStr, c_char};
use std::fs;

pub const COMPACT_OK: i32 = 0;
pub const COMPACT_ERR_NULL_PTR: i32 = 1;
pub const COMPACT_ERR_UNIMPLEMENTED: i32 = 2;
pub const COMPACT_ERR_IO: i32 = 3;
pub const COMPACT_ERR_INVALID_INPUT: i32 = 4;

#[unsafe(no_mangle)]
pub extern "C" fn compact_encode_file(
    input_path: *const c_char,
    schema_path: *const c_char,
    output_path: *const c_char,
) -> i32 {
    if input_path.is_null() || schema_path.is_null() || output_path.is_null() {
        return COMPACT_ERR_NULL_PTR;
    }

    encode_file_with_schema(input_path, schema_path, output_path)
}

#[unsafe(no_mangle)]
pub extern "C" fn compact_decode_file(
    input_path: *const c_char,
    output_path: *const c_char,
) -> i32 {
    if input_path.is_null() || output_path.is_null() {
        return COMPACT_ERR_NULL_PTR;
    }

    let Some(input_path) = c_path_to_string(input_path) else {
        return COMPACT_ERR_INVALID_INPUT;
    };
    let Some(output_path) = c_path_to_string(output_path) else {
        return COMPACT_ERR_INVALID_INPUT;
    };

    let frame = match fs::read(&input_path) {
        Ok(frame) => frame,
        Err(_) => return COMPACT_ERR_IO,
    };
    let config = compact_core::EncodeConfig {
        value_type: compact_core::ValueType::RawBytes,
        transform: compact_core::Transform::None,
        codec: compact_core::Codec::Rle,
    };
    let decoded = match compact_core::decode_bytes_frame(&config, &frame) {
        Ok(decoded) => decoded,
        Err(_) => return COMPACT_ERR_INVALID_INPUT,
    };

    match fs::write(output_path, decoded) {
        Ok(()) => COMPACT_OK,
        Err(_) => COMPACT_ERR_IO,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn compact_decode_file_with_schema(
    input_path: *const c_char,
    schema_path: *const c_char,
    output_path: *const c_char,
) -> i32 {
    if input_path.is_null() || schema_path.is_null() || output_path.is_null() {
        return COMPACT_ERR_NULL_PTR;
    }

    decode_file_with_schema(input_path, schema_path, output_path)
}

fn encode_file_with_schema(
    input_path: *const c_char,
    schema_path: *const c_char,
    output_path: *const c_char,
) -> i32 {
    let Some(input_path) = c_path_to_string(input_path) else {
        return COMPACT_ERR_INVALID_INPUT;
    };
    let Some(schema_path) = c_path_to_string(schema_path) else {
        return COMPACT_ERR_INVALID_INPUT;
    };
    let Some(output_path) = c_path_to_string(output_path) else {
        return COMPACT_ERR_INVALID_INPUT;
    };
    let input = match fs::read_to_string(&input_path) {
        Ok(input) => input,
        Err(_) => return COMPACT_ERR_IO,
    };
    let schema = match load_schema(&schema_path) {
        Ok(schema) => schema,
        Err(status) => return status,
    };
    let encoded = match compact_core::io::encode_jsonl(&input, &schema) {
        Ok(encoded) => encoded,
        Err(_) => return COMPACT_ERR_INVALID_INPUT,
    };

    match fs::write(output_path, encoded) {
        Ok(()) => COMPACT_OK,
        Err(_) => COMPACT_ERR_IO,
    }
}

fn decode_file_with_schema(
    input_path: *const c_char,
    schema_path: *const c_char,
    output_path: *const c_char,
) -> i32 {
    let Some(input_path) = c_path_to_string(input_path) else {
        return COMPACT_ERR_INVALID_INPUT;
    };
    let Some(schema_path) = c_path_to_string(schema_path) else {
        return COMPACT_ERR_INVALID_INPUT;
    };
    let Some(output_path) = c_path_to_string(output_path) else {
        return COMPACT_ERR_INVALID_INPUT;
    };
    let input = match fs::read(&input_path) {
        Ok(input) => input,
        Err(_) => return COMPACT_ERR_IO,
    };
    let schema = match load_schema(&schema_path) {
        Ok(schema) => schema,
        Err(status) => return status,
    };
    let decoded = match compact_core::io::decode_jsonl(&input, &schema) {
        Ok(decoded) => decoded,
        Err(_) => return COMPACT_ERR_INVALID_INPUT,
    };

    match fs::write(output_path, decoded) {
        Ok(()) => COMPACT_OK,
        Err(_) => COMPACT_ERR_IO,
    }
}

fn c_path_to_string(path: *const c_char) -> Option<String> {
    // The C ABI promises a non-null NUL-terminated string. Invalid UTF-8 paths
    // are rejected because the Rust side currently uses `String` paths.
    let path = unsafe { CStr::from_ptr(path) };

    path.to_str().ok().map(ToOwned::to_owned)
}

fn load_schema(path: &str) -> Result<compact_core::schema::Schema, i32> {
    let schema = fs::read_to_string(path).map_err(|_| COMPACT_ERR_IO)?;

    compact_core::schema::Schema::from_yaml(&schema).map_err(|_| COMPACT_ERR_INVALID_INPUT)
}

#[cfg(test)]
mod tests {
    use super::{
        COMPACT_ERR_INVALID_INPUT, COMPACT_ERR_NULL_PTR, COMPACT_OK, compact_decode_file,
        compact_decode_file_with_schema, compact_encode_file,
    };
    use std::ffi::CString;
    use std::fs;

    #[test]
    fn decode_rejects_null_pointers() {
        let status = compact_decode_file(std::ptr::null(), std::ptr::null());
        assert_eq!(status, COMPACT_ERR_NULL_PTR);
    }

    #[test]
    fn decode_rejects_invalid_raw_frame() {
        let input = CString::new("input.cmp").unwrap();
        let output = CString::new("output.jsonl").unwrap();
        let status = compact_decode_file(input.as_ptr(), output.as_ptr());
        assert!(matches!(
            status,
            COMPACT_ERR_INVALID_INPUT | super::COMPACT_ERR_IO
        ));
    }

    #[test]
    fn schema_file_roundtrip() {
        let dir = std::env::temp_dir().join(format!("compact-ffi-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let input_path = dir.join("input.jsonl");
        let schema_path = dir.join("schema.yml");
        let encoded_path = dir.join("encoded.cmp");
        let output_path = dir.join("output.jsonl");

        fs::write(
            &input_path,
            "{\"ts\":100,\"level\":\"INFO\",\"message\":\"hello\"}\n",
        )
        .unwrap();
        fs::write(
            &schema_path,
            "columns:\n  - name: ts\n    type: u64\n    codec: delta_varint_u64\n  - name: level\n    type: string\n    codec: dictionary\n  - name: message\n    type: string\n    codec: dictionary\n",
        )
        .unwrap();

        let input = CString::new(input_path.to_string_lossy().as_bytes()).unwrap();
        let schema = CString::new(schema_path.to_string_lossy().as_bytes()).unwrap();
        let encoded = CString::new(encoded_path.to_string_lossy().as_bytes()).unwrap();
        let output = CString::new(output_path.to_string_lossy().as_bytes()).unwrap();

        assert_eq!(
            compact_encode_file(input.as_ptr(), schema.as_ptr(), encoded.as_ptr()),
            COMPACT_OK
        );
        assert_eq!(
            compact_decode_file_with_schema(encoded.as_ptr(), schema.as_ptr(), output.as_ptr()),
            COMPACT_OK
        );
        assert_eq!(
            fs::read_to_string(output_path).unwrap(),
            fs::read_to_string(input_path).unwrap()
        );
    }
}
