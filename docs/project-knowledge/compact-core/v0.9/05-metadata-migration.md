# Metadata Migration

## Versions

Schema metadata version 1 contains `revision`, physical column names, types,
codecs, and nullability. Version 2 adds `stable_id` to every column and can be
parsed directly as `SchemaRevision`.

The metadata remains an external sidecar for CMP2-CMP4. Writing it into an old
format header would make older readers reject the file and requires a separately
versioned on-disk contract.

## Explicit Identity Assignment

Migration requires one `name=id` assignment for every v1 column. IDs must be
positive and unique. Missing, duplicate, and unknown assignments are rejected.
The migrator never hashes names or uses column positions because rename and
reorder would then change logical identity.

## Preservation and Idempotence

The migration modifies only `metadata_version` and adds `stable_id` fields.
Unknown root and column YAML fields are preserved. The generated document is
validated as a v2 `SchemaRevision`.

Planning an already valid v2 document returns `MigrationAction::None`.
Execution returns the original bytes unchanged, making repeated migration
byte-idempotent.

## Source Binding

Migration plans store source length and CRC32. Execution rejects changed source
bytes, so a dry-run decision cannot be accidentally applied after another
process edits the metadata.

## CLI

```text
compact metadata-migrate schema-v1.yml \
  --column-id id=10 \
  --column-id service=20 \
  --dry-run

compact metadata-migrate schema-v1.yml \
  --column-id id=10 \
  --column-id service=20 \
  --output schema-v2.yml
```

The output path must differ from the source path.
