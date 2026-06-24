# C ABI

The `compact-ffi` crate exposes a C-compatible ABI for language bindings.

## Status Codes

```c
#define COMPACT_OK 0
#define COMPACT_ERR_NULL_PTR 1
#define COMPACT_ERR_UNIMPLEMENTED 2
#define COMPACT_ERR_IO 3
#define COMPACT_ERR_INVALID_INPUT 4
```

Use `compact_status_message(status)` to map a status code to a static
NUL-terminated message. Do not free the returned pointer.

## Version

```c
const char *compact_version(void);
```

The returned pointer is static and must not be freed.

## Owned Buffers

```c
typedef struct CompactBuffer {
    uint8_t *ptr;
    uintptr_t len;
    uintptr_t capacity;
} CompactBuffer;

void compact_buffer_free(CompactBuffer *buffer);
```

Any successful function that writes a `CompactBuffer` transfers ownership of
`ptr` to the caller. The caller must release it with `compact_buffer_free`.
After free, the buffer is reset to null pointer, zero length, and zero capacity.

Foreign bindings should copy the bytes into their own runtime-managed memory
before freeing the Rust buffer.

## Binding Smoke Test

```sh
scripts/cross_language_smoke.sh
```

The smoke test builds the Rust CLI and FFI library, then runs Go, Python, and
Node binding checks against the same local artifacts.

## File APIs

```c
int compact_encode_file(
    const char *input_path,
    const char *schema_path,
    const char *output_path
);

int compact_decode_file_with_schema(
    const char *input_path,
    const char *schema_path,
    const char *output_path
);
```

Paths must be non-null UTF-8 C strings.

## Byte APIs

```c
int compact_encode_bytes_rle(
    const uint8_t *input_ptr,
    uintptr_t input_len,
    CompactBuffer *output
);

int compact_decode_bytes_rle(
    const uint8_t *input_ptr,
    uintptr_t input_len,
    CompactBuffer *output
);
```

`input_ptr` may be null only when `input_len == 0`. `output` must be non-null.
