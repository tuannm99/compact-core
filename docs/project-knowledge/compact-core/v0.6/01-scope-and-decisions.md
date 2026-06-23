# Scope and Decisions

## Goal

v0.6 supports real-time streaming systems and checkpoint-style persistence.
This means files may be written over time and may end with a partial block after
a crash.

## In Scope

- Append-oriented JSONL block streams.
- Sequential replay from append logs.
- Recovery to the last valid block.
- Checkpoint/snapshot compression using existing block codecs.
- Rolling-file design.
- Kafka integration examples.

## Out of Scope

- Direct Kafka client dependency.
- Async runtime dependency.
- Distributed checkpoint coordination.
- Exactly-once semantics.

## Key Decision

Append streams reuse the v0.2 stream header and framed `BLK1` blocks. They do
not write the `IDX1` footer because a footer marks a closed file. Recovery scans
frames from the beginning and stops at the first partial or corrupt record.

This keeps append mode compatible with the current block codec and avoids a new
format before the recovery behavior is proven.

## Implemented Surface

- `streaming::append`
- `streaming::snapshot`
- `streaming::rolling`
- CLI `stream-append`
- CLI `stream-recover`
- CLI `stream-replay`
- CLI `stream-roll`
- CLI `stream-bench`
- CLI `snapshot-encode`
- CLI `snapshot-decode`
