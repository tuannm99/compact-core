# Implementation Plan

## Phase 1 - Compatibility and Validator

Status: implemented.

- Detect CMP1-CMP4 from magic and version.
- Negotiate an explicit reader compatibility range.
- Validate existing formats through one core API.
- Add `compact validate`.
- Test supported versions and representative corruption.

## Phase 2 - Schema Evolution

Status: implemented with externally supplied schema revisions. Embedding schema
identity is deferred to Phase 4 because CMP2-CMP4 have no versioned identity
contract.

- Stable column identity is independent from display order and physical name.
- Add, remove, rename, nullability, codec, type, and default rules are checked.
- Evolved decode supports CMP2, CMP3, and CMP4.
- Compatibility decisions and CLI integration are tested.

## Phase 3 - Recovery and Repair

Status: implemented for recoverable CMP2 and CMP4 boundaries.

- CMP2 recovers only the contiguous prefix of independently checksummed blocks.
- CMP4 reconstructs footer metadata from contiguous checked row groups.
- Repair returns a source-bound plan before writing bytes.
- `compact repair` supports dry-run and requires a different output path.
- Corrupt tails, row groups, footers, and stale plans are tested.

## Phase 4 - Metadata Migration

- Define source and destination metadata versions.
- Add deterministic migration plans and dry-run output.
- Verify idempotence and preserve unknown metadata when safe.

## Phase 5 - Release Hardening

- Build a checked-in compatibility fixture matrix.
- Add corruption simulation and repair benchmarks.
- Publish limits, recovery guarantees, and benchmark results.
