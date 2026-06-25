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
- v0.6 append stream throughput
- v0.6 append recovery latency
- v0.6 replay throughput
- v0.6 snapshot compression ratio

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

Current v0.6 append-stream signals are exposed through:

```sh
compact stream-bench input.jsonl --schema schema.yml --block-rows 10000
```

It reports append throughput, recovery latency, replay throughput, encoded
bytes, and compression ratio.

Current v0.8 parallel encode scaling signals are exposed through:

```sh
compact parallel-bench input.jsonl --schema schema.yml --workers 4 --block-rows 10000
```

It compares sequential CMP2 encode throughput with the v0.8 parallel block
encoder, verifies the parallel output decodes back to the original JSONL, and
reports speedup. The v0.8 release benchmark used 5,000,000 generated rows
(`260,500,000` input bytes), 500 CMP2 blocks, and 16 workers. It produced
`121,594,522` encoded bytes, `6.288x` encode speedup, and `2.956x` decode
speedup versus the sequential CMP2 path on the same run.

For the full run/test/report procedure, see
[`docs/v0.8-benchmark-guide.md`](../docs/v0.8-benchmark-guide.md).
