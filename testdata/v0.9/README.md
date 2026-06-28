# v0.9 Compatibility Fixtures

These checked-in text fixtures keep schema compatibility decisions reproducible.

- `writer-v1.yml`: physical schema used by legacy CMP2-CMP4 files.
- `reader-compatible-v2.yml`: rename, codec change, defaulted add, and drop.
- `reader-incompatible-v2.yml`: type change and tightened nullability.
- `metadata-v1.yml`: name-based metadata migration input with unknown fields.

Binary CMP files are generated from these schemas in tests so checksums and
offsets cannot become stale after an intentional encoder change.
