# Implementation Plan

## Phase 1 - Parallel CMP2 Encode

Status: implemented.

- Add `compact_core::parallel::ParallelOptions`.
- Add `compact_core::parallel::encode_jsonl_stream_parallel`.
- Reuse streaming writer internals for CMP2 header, block payload, and footer.
- Preserve ordered output with a collector keyed by `block_index`.
- Add CLI `compact parallel-bench`.
- Add tests for roundtrip, ordering, invalid worker count, and worker errors.

## Phase 2 - Parallel Decode

Status: implemented.

- Split reader into frame extraction and block payload decode.
- Read frames sequentially to preserve corruption boundaries.
- Decode validated column-block payloads on worker threads.
- Write decoded JSONL in block order.
- Add tests for ordering and corruption handling.

## Phase 3 - Stress And Scaling Benchmarks

Status: implemented.

- Add larger generated JSONL benchmark fixtures.
- Compare 1, 2, 4, and available CPU worker counts.
- Record throughput and speedup in release notes.
- Run repeated stress tests to catch scheduler races.

## Phase 4 - Async I/O Foundation

Status: implemented.

- Define async writer requirements separately from the sync worker scheduler.
- Avoid adding an async runtime to core unless the API needs one.
- Prefer adapter crates or feature gates if runtime-specific integration becomes
  necessary.
