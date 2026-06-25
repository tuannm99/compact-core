# Scope and Decisions

## Goal

Increase encode throughput by running independent CMP2 block compression jobs on
multiple CPU cores while preserving deterministic output order and existing
decode compatibility.

## In Scope

- Parallel CMP2 JSONL block encode.
- Worker pool and block scheduler.
- Ordered collector that writes frames and footer metadata in block order.
- CLI benchmark that compares sequential CMP2 encode with parallel CMP2 encode.
- Tests for roundtrip compatibility, ordering, invalid configuration, and worker
  error propagation.

## Out of Scope

- SIMD-specific codecs. The scheduler keeps block jobs isolated so SIMD can be
  added inside codecs later without changing scheduling.
- New file format. v0.8 phase 1 writes normal CMP2 streams.

## Format Decision

Do not introduce `CMP8` for phase 1. Parallelism is an execution strategy, not a
storage-format change. Existing CMP2 readers, inspectors, and FFI paths should
continue to decode files produced by the parallel encoder.
