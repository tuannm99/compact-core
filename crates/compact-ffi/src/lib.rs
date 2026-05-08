use std::ffi::c_char;

pub const COMPACT_OK: i32 = 0;
pub const COMPACT_ERR_NULL_PTR: i32 = 1;
pub const COMPACT_ERR_UNIMPLEMENTED: i32 = 2;

#[unsafe(no_mangle)]
pub extern "C" fn compact_encode_file(
    input_path: *const c_char,
    schema_path: *const c_char,
    output_path: *const c_char,
) -> i32 {
    if input_path.is_null() || schema_path.is_null() || output_path.is_null() {
        return COMPACT_ERR_NULL_PTR;
    }

    let _ = compact_core::crate_version();
    COMPACT_ERR_UNIMPLEMENTED
}

#[unsafe(no_mangle)]
pub extern "C" fn compact_decode_file(
    input_path: *const c_char,
    output_path: *const c_char,
) -> i32 {
    if input_path.is_null() || output_path.is_null() {
        return COMPACT_ERR_NULL_PTR;
    }

    let _ = compact_core::crate_version();
    COMPACT_ERR_UNIMPLEMENTED
}

#[cfg(test)]
mod tests {
    use super::{COMPACT_ERR_NULL_PTR, COMPACT_ERR_UNIMPLEMENTED, compact_decode_file};
    use std::ffi::CString;

    #[test]
    fn decode_rejects_null_pointers() {
        let status = compact_decode_file(std::ptr::null(), std::ptr::null());
        assert_eq!(status, COMPACT_ERR_NULL_PTR);
    }

    #[test]
    fn decode_placeholder_is_wired() {
        let input = CString::new("input.cmp").unwrap();
        let output = CString::new("output.jsonl").unwrap();
        let status = compact_decode_file(input.as_ptr(), output.as_ptr());
        assert_eq!(status, COMPACT_ERR_UNIMPLEMENTED);
    }
}
