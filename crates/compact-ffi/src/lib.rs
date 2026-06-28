use std::ffi::{CStr, c_char};
use std::fs;
use std::io::Cursor;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

pub const COMPACT_OK: i32 = 0;
pub const COMPACT_ERR_NULL_PTR: i32 = 1;
pub const COMPACT_ERR_UNIMPLEMENTED: i32 = 2;
pub const COMPACT_ERR_IO: i32 = 3;
pub const COMPACT_ERR_INVALID_INPUT: i32 = 4;
pub const COMPACT_ERR_PANIC: i32 = 5;

const STATUS_OK: &[u8] = b"ok\0";
const STATUS_NULL_PTR: &[u8] = b"null pointer\0";
const STATUS_UNIMPLEMENTED: &[u8] = b"unimplemented\0";
const STATUS_IO: &[u8] = b"i/o error\0";
const STATUS_INVALID_INPUT: &[u8] = b"invalid input\0";
const STATUS_PANIC: &[u8] = b"internal panic\0";
const STATUS_UNKNOWN: &[u8] = b"unknown status\0";
const VERSION: &[u8] = concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes();

/// Owned byte buffer returned by the C ABI.
///
/// Callers must treat this as opaque and release it with
/// `compact_buffer_free`. The `capacity` field is part of the ownership
/// contract and must not be modified by foreign callers.
#[repr(C)]
#[derive(Debug)]
pub struct CompactBuffer {
    pub ptr: *mut u8,
    pub len: usize,
    pub capacity: usize,
}

impl Default for CompactBuffer {
    fn default() -> Self {
        Self {
            ptr: ptr::null_mut(),
            len: 0,
            capacity: 0,
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn compact_version() -> *const c_char {
    VERSION.as_ptr().cast()
}

#[unsafe(no_mangle)]
pub extern "C" fn compact_status_message(status: i32) -> *const c_char {
    match status {
        COMPACT_OK => STATUS_OK.as_ptr().cast(),
        COMPACT_ERR_NULL_PTR => STATUS_NULL_PTR.as_ptr().cast(),
        COMPACT_ERR_UNIMPLEMENTED => STATUS_UNIMPLEMENTED.as_ptr().cast(),
        COMPACT_ERR_IO => STATUS_IO.as_ptr().cast(),
        COMPACT_ERR_INVALID_INPUT => STATUS_INVALID_INPUT.as_ptr().cast(),
        COMPACT_ERR_PANIC => STATUS_PANIC.as_ptr().cast(),
        _ => STATUS_UNKNOWN.as_ptr().cast(),
    }
}

#[unsafe(no_mangle)]
/// Free a Rust-owned buffer returned by this ABI.
///
/// # Safety
///
/// `buffer` must be either null or a valid mutable pointer to a `CompactBuffer`
/// previously initialized by this library. If `buffer.ptr` is non-null, it must
/// still be owned by the caller and must not have been freed already.
pub unsafe extern "C" fn compact_buffer_free(buffer: *mut CompactBuffer) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if buffer.is_null() {
            return;
        }

        let buffer = unsafe { &mut *buffer };
        if !buffer.ptr.is_null() && buffer.capacity != 0 {
            let owned = unsafe { Vec::from_raw_parts(buffer.ptr, buffer.len, buffer.capacity) };
            drop(owned);
        }

        *buffer = CompactBuffer::default();
    }));
}

#[unsafe(no_mangle)]
/// Encode bytes into a Rust-owned output buffer.
///
/// # Safety
///
/// `input_ptr` must reference `input_len` readable bytes, unless the length is
/// zero. `output` must point to a valid, empty `CompactBuffer`.
pub unsafe extern "C" fn compact_encode_bytes_rle(
    input_ptr: *const u8,
    input_len: usize,
    output: *mut CompactBuffer,
) -> i32 {
    catch_status(|| unsafe {
        with_input_slice(input_ptr, input_len, output, |input| {
            let config = raw_rle_config();

            compact_core::encode_bytes_frame(&config, input)
        })
    })
}

#[unsafe(no_mangle)]
/// Decode bytes into a Rust-owned output buffer.
///
/// # Safety
///
/// `input_ptr` must reference `input_len` readable bytes, unless the length is
/// zero. `output` must point to a valid, empty `CompactBuffer`.
pub unsafe extern "C" fn compact_decode_bytes_rle(
    input_ptr: *const u8,
    input_len: usize,
    output: *mut CompactBuffer,
) -> i32 {
    catch_status(|| unsafe {
        with_input_slice(input_ptr, input_len, output, |input| {
            let config = raw_rle_config();

            compact_core::decode_bytes_frame(&config, input)
        })
    })
}

#[unsafe(no_mangle)]
/// Encode a JSONL file using a schema file.
///
/// # Safety
///
/// Every argument must be a valid pointer to a NUL-terminated C string for the
/// duration of the call.
pub unsafe extern "C" fn compact_encode_file(
    input_path: *const c_char,
    schema_path: *const c_char,
    output_path: *const c_char,
) -> i32 {
    catch_status(|| {
        if input_path.is_null() || schema_path.is_null() || output_path.is_null() {
            return COMPACT_ERR_NULL_PTR;
        }

        unsafe { encode_file_with_schema(input_path, schema_path, output_path) }
    })
}

#[unsafe(no_mangle)]
/// Decode a raw RLE frame file.
///
/// # Safety
///
/// Every argument must be a valid pointer to a NUL-terminated C string for the
/// duration of the call.
pub unsafe extern "C" fn compact_decode_file(
    input_path: *const c_char,
    output_path: *const c_char,
) -> i32 {
    catch_status(|| {
        if input_path.is_null() || output_path.is_null() {
            return COMPACT_ERR_NULL_PTR;
        }

        let Some(input_path) = (unsafe { c_path_to_string(input_path) }) else {
            return COMPACT_ERR_INVALID_INPUT;
        };
        let Some(output_path) = (unsafe { c_path_to_string(output_path) }) else {
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
    })
}

#[unsafe(no_mangle)]
/// Decode a schema-based file.
///
/// # Safety
///
/// Every argument must be a valid pointer to a NUL-terminated C string for the
/// duration of the call.
pub unsafe extern "C" fn compact_decode_file_with_schema(
    input_path: *const c_char,
    schema_path: *const c_char,
    output_path: *const c_char,
) -> i32 {
    catch_status(|| {
        if input_path.is_null() || schema_path.is_null() || output_path.is_null() {
            return COMPACT_ERR_NULL_PTR;
        }

        unsafe { decode_file_with_schema(input_path, schema_path, output_path) }
    })
}

unsafe fn encode_file_with_schema(
    input_path: *const c_char,
    schema_path: *const c_char,
    output_path: *const c_char,
) -> i32 {
    let Some(input_path) = (unsafe { c_path_to_string(input_path) }) else {
        return COMPACT_ERR_INVALID_INPUT;
    };
    let Some(schema_path) = (unsafe { c_path_to_string(schema_path) }) else {
        return COMPACT_ERR_INVALID_INPUT;
    };
    let Some(output_path) = (unsafe { c_path_to_string(output_path) }) else {
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
    let encoded = match compact_core::streaming::encode_jsonl_stream(
        Cursor::new(input),
        Vec::new(),
        schema,
        compact_core::streaming::BlockOptions::default(),
    ) {
        Ok(encoded) => encoded,
        Err(_) => return COMPACT_ERR_INVALID_INPUT,
    };

    match fs::write(output_path, encoded) {
        Ok(()) => COMPACT_OK,
        Err(_) => COMPACT_ERR_IO,
    }
}

unsafe fn decode_file_with_schema(
    input_path: *const c_char,
    schema_path: *const c_char,
    output_path: *const c_char,
) -> i32 {
    let Some(input_path) = (unsafe { c_path_to_string(input_path) }) else {
        return COMPACT_ERR_INVALID_INPUT;
    };
    let Some(schema_path) = (unsafe { c_path_to_string(schema_path) }) else {
        return COMPACT_ERR_INVALID_INPUT;
    };
    let Some(output_path) = (unsafe { c_path_to_string(output_path) }) else {
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
    let decoded = match compact_core::streaming::decode_jsonl_stream(
        Cursor::new(input),
        Vec::new(),
        schema,
    ) {
        Ok(decoded) => decoded,
        Err(_) => return COMPACT_ERR_INVALID_INPUT,
    };

    match fs::write(output_path, decoded) {
        Ok(()) => COMPACT_OK,
        Err(_) => COMPACT_ERR_IO,
    }
}

unsafe fn with_input_slice<F>(
    input_ptr: *const u8,
    input_len: usize,
    output: *mut CompactBuffer,
    operation: F,
) -> i32
where
    F: FnOnce(&[u8]) -> compact_core::Result<Vec<u8>>,
{
    if (input_ptr.is_null() && input_len != 0) || output.is_null() {
        return COMPACT_ERR_NULL_PTR;
    }

    let input = if input_len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(input_ptr, input_len) }
    };
    let output = unsafe { &mut *output };
    if !output.ptr.is_null() || output.len != 0 || output.capacity != 0 {
        return COMPACT_ERR_INVALID_INPUT;
    }
    let encoded = match operation(input) {
        Ok(encoded) => encoded,
        Err(_) => return COMPACT_ERR_INVALID_INPUT,
    };

    write_output_buffer(output, encoded)
}

fn write_output_buffer(output: &mut CompactBuffer, mut data: Vec<u8>) -> i32 {
    let buffer = CompactBuffer {
        ptr: data.as_mut_ptr(),
        len: data.len(),
        capacity: data.capacity(),
    };
    std::mem::forget(data);

    *output = buffer;

    COMPACT_OK
}

fn raw_rle_config() -> compact_core::EncodeConfig {
    compact_core::EncodeConfig {
        value_type: compact_core::ValueType::RawBytes,
        transform: compact_core::Transform::None,
        codec: compact_core::Codec::Rle,
    }
}

unsafe fn c_path_to_string(path: *const c_char) -> Option<String> {
    // The C ABI promises a non-null NUL-terminated string. Invalid UTF-8 paths
    // are rejected because the Rust side currently uses `String` paths.
    let path = unsafe { CStr::from_ptr(path) };

    path.to_str().ok().map(ToOwned::to_owned)
}

fn catch_status(operation: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(operation)).unwrap_or(COMPACT_ERR_PANIC)
}

fn load_schema(path: &str) -> Result<compact_core::schema::Schema, i32> {
    let schema = fs::read_to_string(path).map_err(|_| COMPACT_ERR_IO)?;

    compact_core::schema::Schema::from_yaml(&schema).map_err(|_| COMPACT_ERR_INVALID_INPUT)
}

#[cfg(test)]
mod tests {
    use super::{
        COMPACT_ERR_INVALID_INPUT, COMPACT_ERR_NULL_PTR, COMPACT_ERR_PANIC, COMPACT_OK,
        CompactBuffer, catch_status, compact_buffer_free, compact_decode_bytes_rle,
        compact_decode_file, compact_decode_file_with_schema, compact_encode_bytes_rle,
        compact_encode_file, compact_status_message, compact_version,
    };
    use std::ffi::{CStr, CString};
    use std::fs;

    #[test]
    fn decode_rejects_null_pointers() {
        let status = unsafe { compact_decode_file(std::ptr::null(), std::ptr::null()) };
        assert_eq!(status, COMPACT_ERR_NULL_PTR);
    }

    #[test]
    fn version_and_status_messages_are_static_c_strings() {
        let version = unsafe { CStr::from_ptr(compact_version()) }
            .to_str()
            .unwrap();
        let message = unsafe { CStr::from_ptr(compact_status_message(COMPACT_OK)) }
            .to_str()
            .unwrap();

        assert_eq!(version, env!("CARGO_PKG_VERSION"));
        assert_eq!(message, "ok");
    }

    #[test]
    fn byte_buffer_api_roundtrips_and_free_resets_buffer() {
        let input = b"aaaaabbbbbcccccccc";
        let mut encoded = CompactBuffer::default();
        let mut decoded = CompactBuffer::default();

        assert_eq!(
            unsafe { compact_encode_bytes_rle(input.as_ptr(), input.len(), &mut encoded) },
            COMPACT_OK
        );
        assert!(!encoded.ptr.is_null());
        assert!(encoded.len > 0);

        assert_eq!(
            unsafe { compact_decode_bytes_rle(encoded.ptr, encoded.len, &mut decoded) },
            COMPACT_OK
        );
        let decoded_slice = unsafe { std::slice::from_raw_parts(decoded.ptr, decoded.len) };
        assert_eq!(decoded_slice, input);

        unsafe {
            compact_buffer_free(&mut encoded);
            compact_buffer_free(&mut decoded);
        }
        assert!(encoded.ptr.is_null());
        assert_eq!(encoded.len, 0);
        assert_eq!(encoded.capacity, 0);
        assert!(decoded.ptr.is_null());
    }

    #[test]
    fn byte_buffer_api_rejects_null_input_with_non_zero_len() {
        let mut output = CompactBuffer::default();
        let status = unsafe { compact_encode_bytes_rle(std::ptr::null(), 1, &mut output) };

        assert_eq!(status, COMPACT_ERR_NULL_PTR);
    }

    #[test]
    fn decode_rejects_invalid_raw_frame() {
        let input = CString::new("input.cmp").unwrap();
        let output = CString::new("output.jsonl").unwrap();
        let status = unsafe { compact_decode_file(input.as_ptr(), output.as_ptr()) };
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
            unsafe { compact_encode_file(input.as_ptr(), schema.as_ptr(), encoded.as_ptr()) },
            COMPACT_OK
        );
        assert_eq!(
            unsafe {
                compact_decode_file_with_schema(encoded.as_ptr(), schema.as_ptr(), output.as_ptr())
            },
            COMPACT_OK
        );
        assert_eq!(
            fs::read_to_string(output_path).unwrap(),
            fs::read_to_string(input_path).unwrap()
        );
    }

    #[test]
    fn byte_buffer_api_rejects_non_empty_output() {
        let input = b"aaaa";
        let mut output = CompactBuffer::default();
        assert_eq!(
            unsafe { compact_encode_bytes_rle(input.as_ptr(), input.len(), &mut output) },
            COMPACT_OK
        );
        assert_eq!(
            unsafe { compact_encode_bytes_rle(input.as_ptr(), input.len(), &mut output) },
            COMPACT_ERR_INVALID_INPUT
        );
        unsafe { compact_buffer_free(&mut output) };
    }

    #[test]
    fn panic_is_contained_as_status() {
        assert_eq!(catch_status(|| panic!("test panic")), COMPACT_ERR_PANIC);
    }
}
