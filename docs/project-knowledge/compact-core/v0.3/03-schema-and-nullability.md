# Schema and Nullability

## Schema Additions

Proposed schema:

```yaml
columns:
  - name: ts
    type: u64
    codec: auto
    nullable: false
  - name: active
    type: bool
    codec: bitmap
    nullable: true
  - name: service
    type: string
    codec: prefix
    nullable: true
```

New value type:

- `bool`

New codec preferences:

- `bitpack`
- `bitmap`
- `prefix`
- `stored`
- `auto`

## Nullable Contract

`nullable` defaults to `false`.

For a required column:

- Missing field is an error.
- Explicit JSON `null` is an error.

For a nullable column:

- Explicit JSON `null` is accepted.
- Missing field must have one documented behavior.

Recommended behavior for v0.3:

```text
missing nullable field == null
```

This behavior must be tested and documented because JSON distinguishes a
missing key from a key containing `null`. Compact output reconstructs the
schema-defined key with a `null` value.

## Validity Bitmap

Nullable columns store one validity bit per row:

```text
1 = non-null value exists
0 = null
```

Only non-null values are passed to the value codec.

Decode procedure:

1. Read and validate the bitmap length.
2. Count set bits.
3. Decode exactly that many non-null values.
4. Merge values with null positions.
5. Reject extra or missing decoded values.

## Implemented Phase 3 Contract

The shared bitmap primitive is implemented in
`crates/compact-core/src/primitives/bitmap.rs`. Boolean column assembly and
validation are implemented in `crates/compact-core/src/codecs/v3/boolean.rs`.

The boolean encoder accepts only `type: bool` with `codec: bitmap`. It emits
`ColumnChunkMetadata` and a payload consumed by the one-shot CMP3 JSONL API in
`crates/compact-core/src/io/v3.rs`. That API supports multiple boolean columns,
schema validation, canonical JSONL reconstruction, and row-group checksums.

The one-shot writer intentionally rejects numeric and string columns until
their CMP3 codecs exist. Multi-row-group streaming and the footer index are
later integration work, not Phase 3 behavior.

## Required Edge Cases

- Empty column.
- All values null.
- No values null.
- Alternating null and non-null.
- First or last value null.
- Nullable string containing an empty string.
- Required field missing.
- Required field explicitly null.
- Nullable field missing.
- Bitmap with non-zero padding bits.
