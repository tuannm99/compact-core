# v0.9 Production Storage

v0.9 hardens existing CMP1-CMP4 files instead of introducing another data
layout. The work is split so repair and migration are built only after the
validator can prove which bytes are trustworthy.

1. [Scope and decisions](01-scope-and-decisions.md)
2. [Compatibility and validation](02-compatibility-and-validation.md)
3. [Schema evolution](03-schema-evolution.md)
4. [Recovery and repair](04-recovery-and-repair.md)
5. [Metadata migration](05-metadata-migration.md)
6. [Release hardening](06-release-hardening.md)
7. [Implementation plan](07-implementation-plan.md)

Phases 1 through 5 are implemented. The delivered APIs cover format detection,
validation, checked schema evolution, copy-on-write recovery, deterministic
external metadata migration, compatibility/corruption matrices, and related CLI
tools.
