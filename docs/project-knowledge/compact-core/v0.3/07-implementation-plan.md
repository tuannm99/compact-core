# v0.3 Implementation Plan

## Phase 1: Format and Schema

Status: implemented and committed.

- Add `CMP3` and version constants. Done.
- Define versioned column chunk metadata. Done.
- Extend schema with `bool`, `nullable`, and new codec preferences. Done.
- Add format parser tests before codec integration. Done.

Exit criteria:

- Empty `CMP3` stream parses. Done.
- Unknown versions and codec IDs fail safely. Done.
- Existing `CMP2` behavior is unchanged. Done through existing regression tests.

## Phase 2: Numeric Bit Packing

Status: implemented and committed.

- Integrate the existing bitpack primitive into numeric columns. Done.
- Persist bit width. Done.
- Add delta-bitpack candidate. Done.
- Add stored numeric fallback. Done.
- Benchmark against delta-varint. Done through deterministic size comparison.

Exit criteria:

- Widths 0 through 64 roundtrip. Done.
- Malformed metadata fails safely. Done.
- Constant/small-delta columns improve or fall back. Done with explicit stored
  fallback; automatic selection remains Phase 6.

## Phase 3: Boolean and Nullability

Status: implemented and committed.

- Add boolean schema type. Done in Phase 1.
- Add value bitmap. Done.
- Add validity bitmap shared by nullable types. Done.
- Define missing nullable field behavior. Done: missing nullable field is null.
- Add null-count metadata. Done.
- Add one-shot CMP3 boolean JSONL integration with row-group checksum. Done.

Exit criteria:

- Required and nullable semantics are fully tested. Done.
- All-null and mixed-null columns roundtrip. Done.
- Multiple boolean columns roundtrip through a CMP3 file. Done.
- Corrupted and truncated row groups fail safely. Done.

## Phase 4: Column Statistics

- Persist counts, sizes, selected codec, and type-specific statistics.
- Extend inspect output.
- Validate statistics against decoded counts.

Exit criteria:

- Inspect reports stats without value decode.
- Corrupt statistics return errors.

## Phase 5: Prefix Strings and Cardinality

- Implement block-local prefix string codec.
- Bound dictionary candidate memory.
- Handle high-cardinality fallback.
- Keep blocks independently decodable.

Exit criteria:

- Prefix-friendly strings improve.
- Random/high-cardinality strings fall back.
- Unicode roundtrips exactly.

## Phase 6: Adaptive Selection

- Add `codec: auto`.
- Evaluate complete bounded block candidates.
- Select by total encoded size.
- Define deterministic tie-breaking.
- Persist actual selected codec.

Exit criteria:

- Decoder never guesses.
- Repeated encoding is byte-stable.
- Stored fallback always exists.

## Phase 7: Benchmark and Release

- Add v0.2/v0.3/gzip benchmark report.
- Extend CLI integration tests.
- Extend fuzzing to `CMP3`.
- Add `docs/v0.3.md`.
- Bump workspace to `0.3.0`.

Exit criteria:

- v0.3 DoD is checked explicitly.
- Full workspace validation passes.
- Release limitations are documented.

## Commit Boundaries

Recommended commits:

```text
docs: define v0.3 column format
feat: add cmp3 column metadata
feat: add numeric bitpack columns
feat: add nullable boolean columns
feat: add column statistics
feat: add prefix string codec
feat: add adaptive column codec selection
test: add v0.3 benchmark and corruption coverage
chore: release v0.3.0
```
