# v0.2 Streaming Knowledge

v0.2 moves `compact-core` from one-shot compression to bounded-memory
streaming compression.

Current v0.1 model:

```text
read whole input
parse whole JSONL
encode all columns
write one frame
```

Target v0.2 model:

```text
read some rows
encode a block
write the block
repeat until EOF
write or expose block metadata
```

The key design constraint:

```text
memory usage should be bounded by block size, not file size
```

## Files

- [01-concepts.md](01-concepts.md): streaming basics and terminology.
- [02-block-sizing.md](02-block-sizing.md): row/byte block limits.
- [03-jsonl-parsing.md](03-jsonl-parsing.md): streaming JSONL parsing.
- [04-reader-writer-api.md](04-reader-writer-api.md): writer, reader, row iterator APIs.
- [05-file-layout.md](05-file-layout.md): file headers, block frames, indexes.
- [06-corruption-memory.md](06-corruption-memory.md): corruption isolation, backpressure, memory budget.
- [07-cli-inspect-bench.md](07-cli-inspect-bench.md): CLI, inspect, benchmark behavior.
- [08-testing-plan.md](08-testing-plan.md): required tests and malformed cases.
- [09-implementation-plan.md](09-implementation-plan.md): implementation order and design decisions.
