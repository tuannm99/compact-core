# Reader and Writer APIs

## Streaming Writer

The writer owns output state.

Recommended sync API:

```rust
pub struct JsonlBlockWriter<W: Write> {
    writer: W,
    schema: Schema,
    options: BlockOptions,
    current: RowGroupBuffer,
    blocks_written: u64,
}
```

Core methods:

```rust
impl<W: Write> JsonlBlockWriter<W> {
    pub fn new(writer: W, schema: Schema, options: BlockOptions) -> Result<Self>;
    pub fn write_jsonl_line(&mut self, line: &str) -> Result<()>;
    pub fn flush_block(&mut self) -> Result<()>;
    pub fn finish(self) -> Result<W>;
}
```

Expected behavior:

- `write_jsonl_line` parses one row and buffers column values.
- It automatically calls `flush_block` when limits are reached.
- `flush_block` encodes the current row group and writes one framed block.
- `finish` flushes remaining rows and writes footer/index if the format uses one.

Do not expose partially initialized output as success.

If `finish` fails, the output file should be considered incomplete.

## Streaming Reader

The reader owns input state.

Recommended sync API:

```rust
pub struct JsonlBlockReader<R: Read> {
    reader: R,
    schema: Schema,
    next_block_index: u64,
}
```

Core methods:

```rust
impl<R: Read> JsonlBlockReader<R> {
    pub fn new(reader: R, schema: Schema) -> Result<Self>;
    pub fn next_block(&mut self) -> Result<Option<DecodedBlock>>;
}
```

Where:

```rust
pub struct DecodedBlock {
    pub block_index: u64,
    pub row_count: usize,
    pub jsonl: String,
}
```

This is a simple first reader.

Later, replace `jsonl: String` with row iterators or column iterators to reduce
allocations further.

## Row Iterator

The v0.2 DoD says streaming decode should support sequential scan.

The simplest scan API:

```rust
while let Some(block) = reader.next_block()? {
    output.write_all(block.jsonl.as_bytes())?;
}
```

More advanced API:

```rust
for row in reader.rows() {
    let row = row?;
    output.write_all(row.as_bytes())?;
}
```

The block API is easier to implement first.

The row iterator can be built on top of the block reader later.
