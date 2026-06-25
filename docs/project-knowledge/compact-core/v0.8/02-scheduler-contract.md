# Scheduler Contract

The parallel encoder has three stages:

```text
input reader -> row-group scheduler -> worker pool -> ordered collector -> CMP2 writer
```

## Scheduler

- Reads JSONL one line at a time.
- Normalizes accepted rows to exactly one trailing newline.
- Applies the same `BlockOptions` limits as the sequential CMP2 writer.
- Assigns each row group a `block_index`, `first_row_index`, and owned JSONL
  payload.

## Worker Pool

- Each worker owns a cloned schema.
- Each job encodes exactly one row group with the existing column-block encoder.
- Worker output contains an encoded CMP1 frame plus enough metadata for the
  collector to write the `IDX1` footer.
- Worker errors are sent back as `Result` values. They must not be swallowed.

## Ordered Collector

Workers may finish out of order. The collector buffers completed jobs in a
`BTreeMap` keyed by `block_index` and writes only when the next expected block is
available.

This preserves physical frame order, block metadata order, footer index order,
and deterministic decode output.

## Safety Rules

- `worker_count == 0` is invalid.
- Output uses the existing CMP2 header, block frames, and footer index.
- If any worker returns an error, the API returns an error. The caller must treat
  the output writer as incomplete.
- No unsafe Rust is needed for phase 1.

## Parallel Decode

Parallel decode scans frames sequentially first. This validates frame checksums,
block indexes, first-row indexes, and footer metadata before payload decode is
dispatched. Workers decode column-block payloads independently. The collector
writes JSONL only when the next expected block is ready.

## Async Writer Contract

Core exposes `AsyncJsonlSink` instead of depending on a runtime. Runtime-specific
crates can implement the trait for Tokio, async-std, or custom backpressure
writers without changing the core scheduler.
