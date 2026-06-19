# v0.4 Queryable Columnar Format

v0.4 turns the columnar file into a queryable format. The core idea is simple:
data stays in compressed row groups, while an EOF footer tells readers exactly
where each row group and column chunk lives.

Read the documents in this order:

1. [Scope and decisions](01-scope-and-decisions.md)
2. [CMP4 format](02-cmp4-format.md)
3. [Index and offsets](03-index-and-offsets.md)
4. [Projection and scan API](04-projection-and-scan-api.md)
5. [Predicate pushdown](05-predicate-pushdown.md)
6. [Testing and benchmarks](06-testing-and-benchmarks.md)
7. [Implementation phases](07-implementation-plan.md)

The main release rule is:

```text
query planning must use metadata first; payload decode is only for selected
row groups and selected columns
```

This keeps v0.4 compatible with streaming blocks while adding partial reads.
