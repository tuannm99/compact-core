# Statistics and Adaptive Selection

## Per-Column Statistics

Persist statistics needed by inspect and future query planning:

- Value count.
- Null count.
- Raw size.
- Compressed size.
- Selected codec.
- Numeric minimum and maximum.
- String distinct count.
- Dictionary entry count when used.
- Bit width when used.

Do not persist statistics that are not validated or tested.

## Candidate Planning

The encoder builds candidate encodings for a column block.

Numeric candidates:

- Delta-varint.
- Delta-bitpack.
- Stored fixed-width.

String candidates:

- Dictionary.
- Prefix.
- RLE string framing where applicable.
- Stored length-prefixed strings.

Boolean candidate:

- Bitmap.

## Deterministic Selection

Select the smallest total encoded size.

Tie-breaking must be stable:

```text
stored < existing stable codec < new experimental codec
```

A stable tie-break prevents identical input from producing different files
after hash-map iteration or implementation refactors.

## Sampling

For the first v0.3 implementation, prefer evaluating the complete block rather
than sampling. Blocks are already bounded by `BlockOptions`, and complete
evaluation avoids sample bias.

Sampling may be added later if candidate generation becomes too expensive.

## Memory Bound

Do not retain every candidate payload simultaneously for large string columns.

Recommended approach:

1. Build one candidate.
2. Compare with the current best.
3. Drop the losing payload.
4. Stop a candidate early when its size already exceeds the current best.

## Inspect Output

Example:

```text
column name=ts type=u64 codec=delta_bitpack rows=10000 nulls=0 raw=80000 compressed=9210 min=1700000000 max=1700009999 bit_width=14
column name=service type=string codec=dictionary rows=10000 nulls=12 raw=92000 compressed=15400 distinct=43 dictionary_entries=43
```

Inspect must parse metadata without decoding all values.

