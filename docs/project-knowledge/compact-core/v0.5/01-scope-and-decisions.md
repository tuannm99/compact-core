# Scope and Decisions

## Goal

v0.5 targets search-index compression. The core data structure is an inverted
index:

```text
term -> posting list
posting -> doc_id + positions
```

The first implementation phase builds a reusable posting-list codec. A later
phase can wrap many posting lists in a term dictionary block.

## In Scope

- Strictly increasing docID lists.
- Delta-varint encoded docIDs.
- Per-document position counts.
- Delta-varint encoded positions.
- Serialized skip entries for seek-oriented scans.
- Checked binary payloads that reject corruption.

## Out of Scope for Phase 1

- Full text tokenization.
- Ranking or scoring.
- Top-k query planner.
- Multi-term boolean query execution.
- Persistent term dictionary block.
- Entropy coding.

## Important Decisions

DocIDs are unsigned `u64` values. They must be strictly increasing because an
inverted index stores each matching document once per term.

Positions are unsigned `u64` values. They must be strictly increasing inside a
single posting because duplicate or decreasing positions make phrase queries
ambiguous.

Skip entries are serialized with byte offsets into each section. This keeps the
format ready for random seek without requiring the first phase to implement a
full index file.

The decoder validates checksums, section boundaries, varint canonical form,
docID ordering, and position ordering. Corrupt input must return an error, not
panic.
