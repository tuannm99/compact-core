# v1.0 Production Readiness

v1.0 is the stabilization release. It should not introduce a new format version
unless the audit proves that an existing CMP1-CMP4 contract is unsafe or
impossible to stabilize.

The main release rule is:

```text
stable contracts first, automation second, new features last
```

1. [Scope and decisions](01-scope-and-decisions.md)
2. [Stabilization audit](02-stabilization-audit.md)
3. [Implementation plan](03-implementation-plan.md)
4. [Phase 1 audit report](04-phase1-audit-report.md)

The first task after v0.9.1 is the stabilization audit. That audit decides which
APIs, formats, CLI commands, and binding behaviors are stable enough to support
after v1.0.
