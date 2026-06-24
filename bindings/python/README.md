# Python binding

Standard-library `ctypes` wrapper over the stable C ABI. Set
`COMPACT_FFI_LIB` when the dynamic library is not in the default target path.

```python

def encode_file(input_path, schema_path, output_path) -> None: ...
def decode_file(input_path, schema_path, output_path) -> None: ...
def version() -> str: ...
def encode_bytes_rle(data: bytes) -> bytes: ...
def decode_bytes_rle(data: bytes) -> bytes: ...

```

Rust-owned byte buffers must be copied into Python `bytes`, then released with
`compact_buffer_free`.
