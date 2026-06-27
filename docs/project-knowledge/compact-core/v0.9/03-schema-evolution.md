# Schema Evolution

## Why Revisions Are External

CMP2-CMP4 do not persist a stable schema ID. Reusing their existing header
extension without a new metadata contract would make v0.8 readers reject new
files and could blur the distinction between CMP4 and a new format version.
Phase 2 therefore keeps `SchemaRevision` external and requires both the writer
and reader revisions during evolved decode. Metadata embedding belongs to the
versioned migration work in Phase 4.

## Stable Identity

Each logical column has a positive `stable_id`. Names may change; IDs may not be
reused. Matching by ID prevents an unrelated column that happens to reuse an old
name from receiving old data. Aliases document prior names but are not used to
override conflicting stable identities.

## Compatibility Rules

- Rename: compatible when the stable ID and value type are unchanged.
- Codec change: compatible because the writer revision selects the physical
  decoder and the reader codec only describes future writes.
- Required to nullable: compatible.
- Nullable to required: incompatible because existing rows may contain null.
- Add nullable column: compatible; old rows receive null.
- Add required column with a typed non-null default: compatible.
- Add required column without a default: incompatible.
- Remove column: compatible; old values are dropped from reader output.
- Change value type: incompatible; no implicit numeric/string coercion occurs.
- Read a newer writer revision with an older reader revision: rejected.

## APIs and CLI

- `schema::evolution::assess` returns concrete actions and issues.
- `schema::evolution::decode_jsonl` supports CMP2, CMP3, and CMP4.
- `compact schema-check <writer> <reader>` prints the plan and exits non-zero
  for incompatible revisions.
- `compact evolve-decode <input> <output> --writer-schema ... --reader-schema ...`
  applies a checked plan.

Schema evolution validates logical compatibility. Storage corruption must still
be checked by the underlying decoder or `compact validate`.
