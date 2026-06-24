# C ABI Contract

The ABI must remain small and explicit:

- No Rust panics may cross the boundary.
- Null pointers return `COMPACT_ERR_NULL_PTR`.
- Invalid UTF-8 paths return `COMPACT_ERR_INVALID_INPUT`.
- Rust-owned buffers must be released with `compact_buffer_free`.
- Static strings returned by `compact_version` and `compact_status_message`
  must not be freed.

See [`../../../ffi.md`](../../../ffi.md) for the public ABI reference.
