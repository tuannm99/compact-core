# Binding Plan

## Go

Go uses cgo. Returned Rust buffers are copied with `C.GoBytes`, then freed with
`compact_buffer_free`.

## Python

Python should use `ctypes` first. The wrapper should copy returned bytes into
Python `bytes`, then call `compact_buffer_free`.

## Node

The current Node wrapper is dependency-free and shells out to the `compact` CLI.
This keeps the SDK usable without a native addon dependency. A later native
binding can wrap the C ABI directly and preserve the same JavaScript API.

## Cross-Language Tests

The compatibility fixture in `scripts/cross_language_smoke.sh` should:

1. Encode bytes with Rust.
2. Decode with Go.
3. Encode with Go.
4. Decode with Rust.
5. Repeat for Python and Node when implemented.
