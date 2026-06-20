# v0.5 Search Engine Compression Integration

v0.5 adapts `compact-core` for inverted-index workloads. The first rule is:
posting-list correctness comes before full search-engine integration.

Read the documents in this order:

1. [Scope and decisions](01-scope-and-decisions.md)
2. [Posting-list format](02-posting-list-format.md)
3. [Implementation phases](03-implementation-plan.md)
4. [Dictionary and query API](04-dictionary-and-query-api.md)
5. [Testing and benchmarks](05-testing-and-benchmarks.md)

The main release rule is:

```text
docID and position streams must be independently validated before higher-level
term dictionary or top-k query code trusts them
```
