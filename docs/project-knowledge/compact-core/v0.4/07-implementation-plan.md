# Implementation Phases

## Phase 1 - CMP4 Footer Foundation

Status: implemented in `format::v4`.

- Add CMP4 constants.
- Add CMP4 header encode/decode.
- Add footer index structs.
- Add EOF trailer encode/decode.
- Validate ranges, checksums, duplicate columns, and row-group ordering.
- Add binary search helper for logical row lookup.

## Phase 2 - Multi-row-group Writer and Reader

Status: implemented in `io::v4` core APIs.

- Write CMP4 row groups using existing column codecs.
- Record absolute row group and column ranges.
- Decode full CMP4 files.
- Preserve v0.3 lossless roundtrip behavior.

## Phase 3 - Column Projection

Status: implemented in `io::v4::decode_jsonl_projected` and
`io::v4::scan_jsonl`.

- Add scan planning for selected columns.
- Read only projected column payloads.
- Add tests proving unselected payloads are not decoded.

## Phase 4 - Predicate Pushdown

Status: implemented for basic `u64` comparisons and `IS NULL` row-group
pruning in `io::v4::scan_jsonl`.

- Decode persisted statistics into comparable values.
- Plan row group skipping.
- Guarantee missing statistics fall back to scanning.

## Phase 5 - Metadata Inspect

Status: implemented. Core footer loading exists as `io::v4::inspect_footer`,
and CLI `compact inspect` prints CMP4 row-group and column ranges.

- Expose footer-only inspect.
- Show row group ranges, column ranges, statistics, and pruning metadata.

## Phase 6 - Stable Scan API

Status: implemented as an initial core API. `scan_jsonl` accepts projection and
typed predicates, returns JSONL plus scan/prune counters, and rejects unknown
columns or invalid predicate types.

- Stabilize projection and predicate API.
- Add clear errors for unknown columns and unsupported predicates.
- Keep API FFI-friendly.

## Phase 7 - Bench, Fuzz, Release

Status: partially implemented. CLI `bench --format v4` reports query signals,
`fuzz/fuzz_targets/cmp4_decode.rs` covers malformed CMP4 entry points, and
release notes exist in `docs/v0.4.md`.

- Add query benchmarks.
- Fuzz footer/trailer decode.
- Update release notes and DoD.
