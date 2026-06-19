# Milestones

This document tracks the delivery milestones for `compact-core`. Each milestone is aligned with the roadmap and Definition of Done in [definition-of-done.md](/home/minhtuan/dev/local/nova-world/compact-core/docs/definition-of-done.md).

## Phase 0 - Foundation

### Deliverables

- Rust workspace: `compact-core`, `compact-cli`, `compact-ffi`
- Base directories: `bindings/go`, `schemas`, `testdata`, `benches`, `docs`
- CLI and FFI scaffolding for future integration

### Exit Criteria

- Workspace layout is stable
- Core crates build successfully
- Basic CLI entrypoint exists
- Basic FFI boundary exists
- Repository structure supports upcoming milestones

## v0.1 - Primitive Compression Core MVP

### Goal

Build the first end-to-end offline compression pipeline for JSONL data.

### Scope

- Single-process
- Single-file
- Offline encode/decode
- Columnar blocks
- JSONL input

### Key Features

- Varint
- ZigZag
- Delta encoding
- RLE
- Dictionary encoding
- Frame format
- CRC32 checksum
- JSONL -> compact file
- compact -> JSONL

### Exit Criteria

- `compact encode/decode` runs end to end
- JSONL roundtrip is exact and byte-equivalent
- At least 4 codecs implemented
- Corruption detection via checksum
- `compact inspect` works
- Compression ratio report exists
- Benchmark command exists
- Basic Go binding works
- 80%+ unit test coverage for core crate
- Fuzz test exists for frame decoder
- Corrupted file never panics
- Invalid frame handled safely
- CI passes on Linux and macOS
- No unsafe Rust outside the FFI layer

## v0.2 - Streaming Block Engine

Status: release hardening. The sequential `CMP2` block stream can encode,
decode, inspect, and benchmark schema-based JSONL without full-file buffering in
the CLI. Release notes are documented in [v0.2.md](v0.2.md). The remaining
release task is optional manual 10 GB scale validation.

### Goal

Support large files with bounded memory and streaming encode/decode flows.

### Scope

- Bounded memory
- Large files
- Stream processing

### Key Features

- Streaming writer: implemented for schema-based JSONL
- Streaming reader: implemented for sequential decode
- Configurable block size: implemented with `--block-rows` and `--block-bytes`
- Incremental flush: implemented by row and byte limits
- Partial decode
- Row iterator: not implemented yet
- Column chunk metadata: not implemented yet beyond existing column-block
  payload metadata

### Exit Criteria

- Encode/decode a 10 GB file without OOM: pending manual 10 GB validation;
  automated generated JSONL coverage exists at smaller scale
- Memory stays under a configurable limit: partially implemented by block
  options, pending measured validation
- Streaming decode supports sequential scan: implemented
- Partial block corruption is isolated: implemented at frame/block checksum
  boundary for sequential decode
- Block index implemented: persisted sequential `IDX1` footer index
- `compact inspect` shows block metadata: implemented for `CMP2`
- Throughput benchmark included: implemented in `compact bench`
- Decode throughput is higher than encode throughput
- Backpressure-safe writer API: implemented for sync `Write`
- No full-file buffering: implemented for CLI schema encode/decode path
- All codecs support streaming mode: implemented for current schema JSONL
  column-block path only
- Benchmarks are reproducible: implemented with explicit block size flags
- CLI integration coverage: implemented for streaming encode, decode, inspect,
  bench, block sizing, and invalid block options
- Generated JSONL validation: implemented for 10,000 rows across 10 blocks

## v0.3 - Advanced Column Compression

Status: implemented for stable v0.3. See [v0.3.md](v0.3.md) and the
[benchmark report](v0.3-benchmarks.md).

### Goal

Improve compression ratio with more capable column-aware codecs.

### Scope

- Columnar optimization
- High compression ratio

### Key Features

- Bit packing
- Boolean bitmap compression
- Prefix string compression
- Nullable column support
- Adaptive codec selection
- Column statistics

### Exit Criteria

- Adaptive codec chooser works
- Compression ratio beats `gzip` on structured logs
- Prefix compression implemented
- Nullable columns supported
- Per-column statistics stored
- Dictionary reuse supported
- `compact inspect` shows column stats
- Compression benchmark report generated
- Configurable codec pipeline
- Codec fallback supported
- Large-cardinality strings handled
- Decode remains lossless

## v0.4 - Queryable Columnar Format

Status: release-ready pending commit/tag. CMP4 now has a
footer/index foundation, multi-row-group JSONL encode/decode, column projection,
basic predicate pushdown over persisted statistics, CLI integration,
metadata-only inspect, benchmark signals, fuzz target coverage, and release
notes in [v0.4.md](v0.4.md). Longer fuzz runs and lower-noise Criterion query
benchmarks are tracked as post-release hardening.

### Goal

Enable partial reads and query-oriented scans over compressed data.

### Scope

- Analytics-friendly format
- Partial reads

### Key Features

- Sparse index
- Min/max metadata
- Predicate pushdown
- Column projection
- Row group pruning
- Fast metadata scan

### Exit Criteria

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

### Goal

Adapt the format and codecs for search-index compression workloads.

### Scope

- Search/index compression

### Key Features

- Posting list compression
- Delta docID encoding
- Position compression
- Term dictionary block
- Skip pointers

### Exit Criteria

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

### Goal

Support append-oriented streaming systems and checkpoint-style persistence.

### Scope

- Kafka
- Streaming
- Checkpointing

### Key Features

- Streaming snapshot compression
- Window state compression
- Incremental checkpoint
- Append-only block mode
- Rolling file support

### Exit Criteria

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

### Goal

Expose the format safely across multiple language runtimes.

### Scope

- Cross-language ecosystem

### Key Features

- Stable C ABI
- Go SDK
- Python SDK
- Node SDK
- Version compatibility layer

### Exit Criteria

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

### Goal

Scale throughput across CPU cores without sacrificing ordering or safety.

### Scope

- Multi-core
- High throughput

### Key Features

- Parallel block compression
- Parallel decode
- Worker pool
- Async I/O
- SIMD-ready architecture

### Exit Criteria

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

### Goal

Harden the format for compatibility, durability, and recovery scenarios.

### Scope

- Storage durability
- Compatibility
- Recovery

### Key Features

- File footer index
- Schema evolution
- Backward compatibility
- Recovery tool
- Corruption repair
- Metadata migration

### Exit Criteria

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

### Goal

Ship a stable, portable compression platform with production readiness.

### Scope

- Stable
- Portable compression platform

### Key Features

- Stable format spec
- Stable SDK APIs
- Production benchmarks
- Observability
- Documentation
- Release automation

### Exit Criteria

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

## End State

### Product Vision

`compact-core` becomes a portable columnar compression engine.

### Target Use Cases

- Search engine
- Streaming engine
- Log analytics
- Time-series storage
- Distributed snapshots
- Rate limiter persistence
- Checkpointing

### Skills Built Along the Way

- Compression
- Storage engines
- Stream processing
- Indexing
- FFI
- Systems programming
- Performance engineering
- Algorithm intuition
