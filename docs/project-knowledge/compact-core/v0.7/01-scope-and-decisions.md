# Scope and Decisions

## Goal

Provide a stable C ABI and language binding surface for Go, Python, and Node.

## Phase 1 Scope

- Version function. Done.
- Status message function. Done.
- Rust-owned byte buffer with explicit free. Done.
- In-memory RLE byte encode/decode ABI. Done.
- File encode/decode ABI remains supported. Done.
- Memory ownership documentation. Done.
- Go SDK byte API. Done.
- Python SDK byte API. Done.
- Node SDK byte API. Done through CLI-backed wrapper.
- Cross-language smoke script. Done.

## Decisions

The C ABI is the only stable boundary. Language SDKs should wrap C ABI calls and
copy Rust-owned buffers into runtime-owned memory before freeing Rust memory.

The first in-memory API is raw RLE bytes because it is simple and exercises
ownership without mixing schema parsing into every binding.
