# Release Hardening

## Compatibility Fixtures

`testdata/v0.9` stores schema and metadata text fixtures. Tests generate binary
CMP2-CMP4 files at runtime, avoiding stale embedded checksums while preserving
the compatibility decisions across releases.

The matrix covers:

- New readers detecting and validating CMP1-CMP4.
- Compatible schema rename, codec change, defaulted add, nullable add, and drop.
- Rejection of type changes, tightened nullability, missing required defaults,
  duplicate identities, and older reader revisions.
- Metadata v1-to-v2 migration and byte-idempotent v2 no-op.

## Corruption Simulation

Automated tests truncate every supported fixed header below five bytes. CMP4
tests corrupt each row-group payload in turn and assert that repair recovers
exactly the groups before the damaged boundary. Footer corruption, partial CMP2
frames, stale plans, invalid offsets, and trailing bytes are also covered.

## Repair Benchmark

Build release mode and benchmark a deliberately damaged CMP2 or CMP4 file:

```text
cargo build --release -p compact-cli
target/release/compact repair-bench damaged.cmp --iterations 100
```

Report these fields:

- Input and repaired output bytes
- Format and repair action
- Recovered units and rows
- Iteration count
- One-time planning milliseconds
- Total execution milliseconds
- Aggregate execution MiB/s

Use the same damaged fixture and iteration count when comparing commits.
Planning is measured once because production callers review one plan before one
write; repeated execution isolates reconstruction cost for a stable throughput
signal.

## Recorded Release Benchmark

Recorded locally in release mode on 2026-06-27. The generated fixture contained
100,000 JSONL rows, encoded as 100 CMP4 row groups with `--block-rows 1000`.
One byte was removed from the footer before benchmarking.

```text
input_bytes: 460956
output_bytes: 460957
recovered_units: 100
recovered_rows: 100000
iterations: 100
plan_ms: 0.253
execute_ms: 37.337
execute_mib_s: 1177.398
```

The benchmark recovered every row group and rebuilt a byte-complete footer. The
throughput is a local signal, not a cross-machine performance guarantee.

## Release Limits

- CMP1 and CMP3 cannot be partially repaired because each is one checksummed unit.
- CMP2 and CMP4 repair only contiguous authenticated prefixes.
- Schema revisions and migration metadata remain external sidecars.
- Repair returns memory buffers; atomic rename and durability sync are caller policy.
- CRC32 detects accidental corruption but is not cryptographic authentication.
