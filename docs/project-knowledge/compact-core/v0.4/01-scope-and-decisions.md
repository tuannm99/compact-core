# Scope and Decisions

## Goal

v0.4 must support query-oriented reads over compressed columnar files:

- Metadata-only inspection without decoding payloads.
- Column projection, so reading one column does not decode every column.
- Row group pruning, so predicates can skip irrelevant row groups.
- Stable scan APIs that can evolve without changing the on-disk footer.

## Non-goals

- v0.4 is not a distributed query engine.
- v0.4 does not need SQL parsing.
- v0.4 should not rewrite the CMP3 codec layer.
- v0.4 should not require full-file buffering for normal scans.

## Format Decision

CMP4 keeps CMP3-style row groups and adds a footer index at EOF. The footer is
chosen over a header index because streaming writers do not know final offsets
until data is written. A reader can seek to the end, decode the fixed trailer,
then read only the footer metadata.

## API Decision

Expose explicit projection and predicate structures instead of string query
expressions. This keeps the core crate small, testable, and safe for FFI later.
