# Testing and Benchmarks

## Correctness Matrix

Numeric:

- Empty input.
- Constant values.
- Monotonic small deltas.
- Large deltas.
- `0`, `u64::MAX`, and width 64.
- Bit width metadata corruption.
- Non-zero padding.

Boolean:

- All false.
- All true.
- Alternating values.
- Nullable all-null.
- Nullable mixed values.

String:

- Empty strings.
- Duplicate strings.
- Unicode.
- Long common prefix.
- No shared prefix.
- Large-cardinality unique strings.
- Invalid UTF-8 reconstructed payload.

Nullability:

- Missing required fields.
- Explicit null in required fields.
- Missing nullable fields.
- Null count mismatch.
- Validity bitmap padding corruption.

Adaptive selection:

- Expected codec selected for representative datasets.
- Stored fallback when every codec loses.
- Deterministic selection across repeated runs.

## Malformed Input

Every new parser path must test:

- Truncated metadata.
- Length overflow.
- Payload beyond remaining bytes.
- Unknown type or codec ID.
- Invalid bit width.
- Count mismatch.
- Trailing bytes.
- Footer statistics mismatch.

Malformed files must return errors and never panic.

## Benchmark Datasets

- Monotonic timestamps.
- Small-range counters.
- Booleans.
- Low-cardinality log levels.
- Shared-prefix service names and paths.
- High-cardinality UUID-like strings.
- Nullable columns at 0%, 10%, 50%, and 100%.
- Structured JSONL logs.

## Comparisons

Report:

- v0.2 size and throughput.
- v0.3 explicit codec size and throughput.
- v0.3 auto codec size and throughput.
- `gzip` output size for structured logs.

The `gzip` comparison is a release target, not a reason to compromise
correctness or use dataset-specific hacks.

## Release Gates

- Full workspace tests pass.
- Fuzz target covers the new block decoder.
- Generated multi-block CLI roundtrip passes.
- No regression in v0.2 decode compatibility.
- Benchmark report is committed or linked.
- At least one representative structured-log dataset beats `gzip` in ratio.

