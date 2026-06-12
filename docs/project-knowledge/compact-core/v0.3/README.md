# v0.3 Advanced Column Compression

v0.3 improves compression ratio by selecting codecs per column and persisting
the metadata required to decode those choices safely.

Read the documents in this order:

1. [Scope and decisions](01-scope-and-decisions.md)
2. [CMP3 format](02-cmp3-format.md)
3. [Schema and nullability](03-schema-and-nullability.md)
4. [Column codecs](04-column-codecs.md)
5. [Statistics and adaptive selection](05-statistics-and-selection.md)
6. [Testing and benchmarks](06-testing-and-benchmarks.md)
7. [Implementation phases](07-implementation-plan.md)

The main release rule is:

```text
the encoder may choose a codec, but the decoder must never guess
```

Every selected codec and every parameter required for decode must be persisted
in the file.

