# Implementation Phases

## Phase 1: Posting-List Codec

- Add `compact_core::search::postings`. Done.
- Encode/decode `(doc_id, positions)` posting lists. Done.
- Validate docID and position ordering. Done.
- Persist skip entries. Done.
- Add seek-by-docID over the skip table. Done.
- Add corruption tests. Done.

## Phase 2: Term Dictionary Block

- Store many term -> posting-list references. Done.
- Sort terms lexicographically. Done.
- Add binary search by term. Done.
- Reuse posting-list payloads without decoding unrelated terms. Done.

## Phase 3: Query APIs

- Add single-term lookup API. Done.
- Add intersection foundation for AND queries. Done.
- Add phrase-query position checks. Done for adjacent two-term phrase checks.
- Add top-k benchmark scaffolding. Done as `top_k_by_term_frequency`.

## Phase 4: CLI and Benchmarks

- Add inspect output for term count, posting bytes, skip density, and largest
  posting list. Implemented through `search-inspect` for term count, posting
  bytes, and per-term doc counts/ranges.
- Add benchmark fixtures for generated search indexes. Implemented through CLI
  integration coverage with deterministic line-based fixtures.
- Compare raw posting storage against compressed posting-list storage.
  Implemented through `search-bench` input/encoded byte reporting.
