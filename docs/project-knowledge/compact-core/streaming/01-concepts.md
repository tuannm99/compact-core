# Streaming Concepts

Streaming means processing data incrementally instead of loading the entire
input into memory.

One-shot encode:

```text
input file -> Vec/String containing entire file -> encode -> output file
```

Streaming encode:

```text
input reader -> row buffer -> block encode -> output writer
input reader -> row buffer -> block encode -> output writer
...
```

One-shot decode:

```text
input file -> Vec containing entire compact file -> decode -> output string
```

Streaming decode:

```text
input reader -> read one block -> decode rows -> output writer
input reader -> read next block -> decode rows -> output writer
...
```

The caller should not need enough memory to hold a 10 GB input file.

## Terms

Use these words consistently.

### Chunk

A chunk is a temporary piece of input read from I/O.

Example:

```text
read 64 KiB from file
```

A chunk is an I/O concept. It may split a JSONL line in the middle.

### Row Group

A row group is a set of complete logical rows collected for compression.

Example:

```text
rows 0..9999
```

For JSONL, row groups must contain complete lines.

### Column Block

A column block is the encoded representation of one row group after values are
split by column.

Example:

```text
row group:
  {"ts":100,"level":"INFO"}
  {"ts":101,"level":"INFO"}

column block:
  ts     -> [100, 1] -> delta_varint_u64 bytes
  level  -> ["INFO", "INFO"] -> dictionary bytes
```

### Frame

A frame is the integrity envelope around bytes.

v0.1 already has:

```text
[magic][version][codec id][payload length][crc32][payload]
```

For v0.2, each block should be independently framed or independently
checksummed so corruption can be isolated.

Recommended terminology:

```text
chunk      = raw bytes read from I/O
row group  = complete JSONL rows buffered for one encode unit
block      = encoded columnar payload for one row group
frame      = versioned/checksummed envelope around a block
```

## Why Blocks Matter

Blocks make large-file processing possible.

Benefits:

- Bounded memory.
- Partial corruption isolation.
- Progress reporting.
- Parallel compression later.
- Block-level index later.
- Partial reads later.

Tradeoff:

- Smaller blocks use less memory but usually compress worse.
- Larger blocks compress better but use more memory and increase corruption
  blast radius.

Example:

```text
block size = 1,000 rows
10,000,000 rows -> 10,000 blocks
```

If block 4,211 is corrupted, a decoder may still inspect or decode blocks
before and after it if the format supports resynchronization or an index.
