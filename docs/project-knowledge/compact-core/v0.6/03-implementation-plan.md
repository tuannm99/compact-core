# Implementation Phases

## Phase 1: Append Log Core

Status: implemented in `streaming::append`.

- Add `JsonlAppendWriter`.
- Reuse v0.2 block frames.
- Finish without writing `IDX1`.
- Recover valid prefix and block metadata.
- Replay valid prefix sequentially.
- Append after a corrupt or truncated tail by keeping only the valid prefix.

## Phase 2: Checkpoint Snapshots

- Add snapshot helpers for state blobs. Done in `streaming::snapshot`.
- Document checkpoint restore flow. Done in v0.6 docs and CLI.
- Add corruption tests for snapshot replay. Done.

## Phase 3: Rolling Files

- Add rolling policy by max bytes and max blocks. Done.
- Return completed segment payloads. Done.
- Ensure segment boundaries are block boundaries. Done.

## Phase 4: Integration Examples

- Add Kafka-style example without taking a hard Kafka dependency. Done.
- Show append, flush, recover, replay, and checkpoint restore. Done.

## Phase 5: Benchmarks

- Add sustained append throughput signal. Done through `compact stream-bench`.
- Add replay throughput signal. Done through `compact stream-bench`.
- Add compression overhead measurement. Done through encoded bytes and ratio.
