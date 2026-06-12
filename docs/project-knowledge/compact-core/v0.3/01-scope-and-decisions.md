# Scope and Decisions

## Goal

Improve compression ratio for structured JSONL while preserving:

- Lossless JSONL reconstruction.
- Bounded-memory block processing.
- Independently decodable blocks.
- Deterministic encoding decisions.
- Safe handling of malformed input.

## Included

- `CMP3` file format.
- Bit-packed numeric columns.
- Boolean bitmap columns.
- Nullable columns with validity bitmaps.
- Prefix-compressed strings.
- Per-column statistics.
- Adaptive codec selection.
- Stored/raw fallback.
- Configurable explicit or automatic codecs.
- Compression benchmark comparison with v0.2 and `gzip`.

## Deferred

- Predicate pushdown.
- Column projection.
- Sparse indexes.
- Random row-group seek.
- Cross-block dictionary state.
- Async and parallel compression.
- Schema evolution.

Those features belong to later queryable or parallel-storage milestones.

## Format Decision

Use `CMP3` rather than silently changing `CMP2`.

Reasons:

- v0.3 adds new schema types and codec IDs.
- Column metadata becomes part of the decode contract.
- Nullable values change row reconstruction.
- Adaptive encoding means the file stores the actual selected codec, not only
  the schema preference.

`CMP2` files remain readable by the v0.2 path. `CMP3` gets its own parser and
strict version checks.

## Block Independence

Each block must contain everything needed to decode that block:

- Selected codec per column.
- Codec parameters such as bit width.
- Validity bitmap when nullable.
- Local dictionary or prefix state.
- Column lengths and counts.

Do not share dictionaries or prefix state across blocks in v0.3. Cross-block
state improves ratio but makes corruption recovery and independent decode much
harder.

## Required Fallback Rule

Every adaptive codec must compare its output with a stored representation.

```text
if encoded size >= stored size:
    select stored
```

The comparison includes codec-specific metadata. A codec is not useful if its
payload is smaller but its total column chunk is larger.

