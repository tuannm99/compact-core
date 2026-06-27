# Scope and Decisions

## Goal

Make stored files safe to operate across upgrades, corruption, and partial
writes. v0.9 is a hardening release over CMP1-CMP4, not a reason to invent CMP5.

## Decisions

- Format detection uses the four-byte magic plus the persisted version byte.
- A known magic paired with another version is rejected. Guessing a decoder can
  turn corruption into silent data loss.
- Compatibility is an explicit inclusive reader range.
- Validation without a schema checks envelopes, lengths, indexes, ordering, and
  checksums. It does not claim that values satisfy an external schema.
- Repair must write a new output file. It must never mutate the only source copy.
- Migration must be deterministic and decode the source through its owning
  versioned reader before writing destination metadata.

## Non-goals for Phase 1

- Schema evolution rules
- Automatic repair
- Metadata migration
- Compatibility fixture generation
- Repair throughput benchmarks
