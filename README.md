# compact-core

Portable compression core for log archives, search indexes, stream snapshots, and
columnar storage.

## Workspace

```text
compact-core/
├── crates/
│   ├── compact-core/
│   ├── compact-cli/
│   └── compact-ffi/
├── bindings/
│   └── go/
│   └── python/
├── schemas/
├── testdata/
├── benches/
└── docs/
```

## Current status

This repo is initialized for the Phase 0 milestone:

- Rust workspace with core, CLI, and FFI crates
- CLI command surface for `encode`, `decode`, `inspect`, and `bench`
- Core error and format constants scaffolding
- C ABI placeholder functions for future Go binding work
- Sample schema and JSONL input for the column compression MVP

## Next milestones

1. Encoding primitives: varint, zigzag, delta, RLE
2. Codec interface and first codec implementations
3. Versioned frame format with checksum
4. Column compression MVP for JSONL + YAML schema
5. CLI execution path and inspection output
