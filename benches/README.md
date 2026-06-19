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

Current v0.4 query signals are exposed through the CLI benchmark command:

```sh
compact bench input.jsonl --schema schema.yml --format v4 --block-rows 1000
```

It reports row groups, rows, encoded size, full decode throughput, and a
one-column projected decode timing. Criterion benchmarks should replace this
with lower-noise measurements before release hardening.
