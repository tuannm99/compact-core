# Roadmap + Definition of Done

## v0.1 - Primitive Compression Core MVP

### Scope

- Single-process
- Single-file
- Offline encode/decode
- Columnar blocks
- JSONL input

### Features

- Varint
- ZigZag
- Delta encoding
- RLE
- Dictionary encoding
- Frame format
- CRC32 checksum
- JSONL -> compact file
- compact -> JSONL

### Definition of Done

- `compact encode/decode` runs end to end
- JSONL roundtrip is exact and byte-equivalent
- At least 4 codecs implemented
- Corruption detection via checksum
- `compact inspect` works
- Compression ratio report exists
- Benchmark command exists
- Basic Go binding works
- 80%+ unit test coverage for core crate
- Fuzz test exists for frame decoder, documented in [fuzzing.md](fuzzing.md)
- Corrupted file never panics
- Invalid frame handled safely
- CI passes on Linux and macOS
- No unsafe Rust outside the FFI layer

## v0.2 - Streaming Block Engine

Status: implemented for schema-backed JSONL streaming. See [v0.2.md](v0.2.md)
for release notes, supported behavior, and remaining limitations.

### Scope

- Bounded memory
- Large files
- Stream processing

### Features

- Streaming writer: implemented
- Streaming reader: implemented
- Configurable block size: implemented with `--block-rows` and `--block-bytes`
- Incremental flush: implemented
- Partial decode: implemented at block boundaries
- Row iterator: deferred
- Column chunk metadata: deferred beyond current column-block metadata

### Definition of Done

- Encode/decode a 10 GB file without OOM: pending manual validation
- Memory stays under a configurable limit: implemented by block options,
  pending manual measurement at 10 GB scale
- Streaming decode supports sequential scan: implemented
- Partial block corruption is isolated: implemented
- Block index implemented: persisted `IDX1` footer index
- `compact inspect` shows block metadata: implemented
- Throughput benchmark included: implemented
- Decode throughput is higher than encode throughput: measured by benchmark,
  dataset-dependent
- Backpressure-safe writer API: implemented for sync `Write`
- No full-file buffering: implemented for CLI schema encode/decode
- All codecs support streaming mode: implemented for current schema JSONL codecs
- Benchmarks are reproducible: implemented with explicit block size flags

## v0.3 - Advanced Column Compression

Status: implemented. See [v0.3.md](v0.3.md) and
[v0.3-benchmarks.md](v0.3-benchmarks.md).

### Scope

- Columnar optimization
- High compression ratio

### Features

- Bit packing
- Boolean bitmap compression
- Prefix string compression
- Nullable column support
- Adaptive codec selection
- Column statistics

### Definition of Done

- Adaptive codec chooser works: implemented
- Compression ratio beats `gzip` on structured logs: measured
- Prefix compression implemented: implemented
- Nullable columns supported: implemented
- Per-column statistics stored: implemented
- Dictionary reuse supported: implemented within each independently decodable block
- `compact inspect` shows column stats: implemented
- Compression benchmark report generated: implemented
- Configurable codec pipeline: implemented through schema codecs
- Codec fallback supported: implemented with stored candidates
- Large-cardinality strings handled: bounded dictionary with fallback
- Decode remains lossless: covered by core and CLI roundtrip tests

## v0.4 - Queryable Columnar Format

### Scope

- Analytics-friendly format
- Partial reads

### Features

- Sparse index
- Min/max metadata
- Predicate pushdown
- Column projection
- Row group pruning
- Fast metadata scan

### Definition of Done

- Read a single column without full decode
- Predicate pruning works
- Metadata-only scan supported
- Row group skipping works
- Query benchmark exists
- Query latency is measurable
- Block statistics persisted
- Binary search over block index
- Projection reduces I/O
- Compatible with streaming blocks
- Scan API is stable

## v0.5 - Search Engine Compression Integration

### Scope

- Search/index compression

### Features

- Posting list compression
- Delta docID encoding
- Position compression
- Term dictionary block
- Skip pointers

### Definition of Done

- Posting-list compressed format works
- docID decode is correct
- Skip-pointer navigation implemented
- Random seek supported
- Compression ratio benchmarked
- Top-k scan benchmark exists
- Query latency is measurable
- Dictionary block is reusable
- Term lookup complexity documented
- Compatible with a future search engine

## v0.6 - Real-time Streaming Integration

### Scope

- Kafka
- Streaming
- Checkpointing

### Features

- Streaming snapshot compression
- Window state compression
- Incremental checkpoint
- Append-only block mode
- Rolling file support

### Definition of Done

- Streaming append mode is stable
- Rolling blocks supported
- Checkpoint snapshots are compressible
- Recovery flow implemented
- Append corruption is isolated
- Sequential replay works
- Kafka integration example exists
- Streaming benchmark exists
- Sustained-throughput benchmark recorded
- Compression overhead is measurable

## v0.7 - FFI + Multi-language SDK

### Scope

- Cross-language ecosystem

### Features

- Stable C ABI
- Go SDK
- Python SDK
- Node SDK
- Version compatibility layer

### Definition of Done

- C ABI documented
- Go binding is production-usable
- Python binding is usable
- Node binding is usable
- Cross-language compatibility tests pass
- The same file decodes in all SDKs
- ABI backward compatibility tested
- Semantic versioning introduced
- Release artifacts generated automatically
- Example apps exist for all bindings
- FFI memory ownership documented

## v0.8 - Parallel Compression Engine

### Scope

- Multi-core
- High throughput

### Features

- Parallel block compression
- Parallel decode
- Worker pool
- Async I/O
- SIMD-ready architecture

### Definition of Done

- Multi-thread scaling benchmark exists
- Compression scales with CPU cores
- Decode parallelism works
- Ordering preserved
- Thread-safe APIs
- Block scheduler implemented
- Throughput exceeds single-thread version
- Large-file benchmark published
- No race conditions under stress
- Async writer is stable

## v0.9 - Production-grade Storage Format

Status: implemented for v0.9.0. Detection and validation cover CMP1-CMP4;
schema evolution supports external stable-ID revisions across CMP2-CMP4;
copy-on-write repair supports authenticated CMP2 and CMP4 prefixes; metadata
migration upgrades external v1 schemas to stable-ID v2; compatibility,
corruption, CLI, and repair benchmark coverage are included. See
[v0.9.md](v0.9.md).

### Scope

- Storage durability
- Compatibility
- Recovery

### Features

- File footer index
- Schema evolution
- Backward compatibility
- Recovery tool
- Corruption repair
- Metadata migration

### Definition of Done

- Schema evolution supported
- Backward compatibility tests pass
- Recovery CLI exists
- Partial file recovery works
- File validator implemented
- Metadata migration works
- Version negotiation implemented
- Repair benchmark exists
- Corruption simulation suite exists
- Large compatibility matrix tested

## v1.0 - Production Release

### Scope

- Stable
- Portable compression platform

### Features

- Stable format spec
- Stable SDK APIs
- Production benchmarks
- Observability
- Documentation
- Release automation

### Definition of Done

- Full format specification published
- Stable public APIs
- Benchmark suite published
- Docs site complete
- Release CI/CD complete
- Cross-platform builds automated
- Linux, macOS, and Windows supported
- 90%+ test coverage for core
- Fuzz testing integrated into CI
- No known crash bugs
- Memory leak checks pass
- Performance regression tests exist
- Real-world datasets benchmarked
- Used successfully in at least:
  - Log archive
  - Search index
  - Streaming snapshot
  - Analytics workload

## Finished State

### Final vision

`compact-core` becomes a portable columnar compression engine.

### Usable for

- Search engine
- Streaming engine
- Log analytics
- Time-series storage
- Distributed snapshots
- Rate limiter persistence
- Checkpointing

### Teaches

- Compression
- Storage engines
- Stream processing
- Indexing
- FFI
- Systems programming
- Performance engineering
- Algorithm intuition
