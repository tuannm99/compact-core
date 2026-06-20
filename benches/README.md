# Benchmarks

Reserved for Criterion benchmarks:

- encode throughput
- decode throughput
- compression ratio
- allocation tracking
- v0.4 full scan latency
- v0.4 projected scan latency
- v0.4 predicate-pruned scan latency
- v0.4 metadata-only inspect latency
- v0.5 raw posting bytes versus compressed dictionary bytes
- v0.5 single-term lookup latency
- v0.5 term/docID seek latency
- v0.5 AND query latency
- v0.5 top-k term-frequency scan latency

Current v0.4 query signals are exposed through the CLI benchmark command:

```sh
compact bench input.jsonl --schema schema.yml --format v4 --block-rows 1000
```

It reports row groups, rows, encoded size, full decode throughput, and a
one-column projected decode timing. Criterion benchmarks should replace this
with lower-noise measurements before release hardening.

Current v0.5 search signals are exposed through:

```sh
compact search-bench postings.txt --skip-step 16 --top-k 5
```

The input format is documented in
`docs/project-knowledge/compact-core/v0.5/05-testing-and-benchmarks.md`.
