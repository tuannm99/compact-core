# v1.0 Implementation Plan

## Phase 1: Stabilization Audit

- Inventory public Rust APIs and classify stable/internal/experimental.
- Inventory C ABI and binding APIs.
- Inventory CLI commands and documented behavior.
- Inventory CMP1-CMP4 format contracts.
- Compare current behavior with `docs/definition-of-done.md`.

Exit condition:

- A concrete checklist exists for the remaining v1.0 work.
- Any unstable API or format behavior is explicitly marked before stabilization.

## Phase 2: Public Specification

- Write the full CMP1-CMP4 format specification.
- Document compatibility and version negotiation rules.
- Document security limits for counts, sizes, and allocation behavior.
- Link format tests to the spec sections they protect.

Exit condition:

- A reader can implement a compatible decoder from the docs.

## Phase 3: API and SDK Freeze

- Add or complete docs for stable Rust APIs.
- Confirm FFI safety docs cover every unsafe export.
- Confirm Go, Python, and Node bindings expose only stable behavior.
- Add missing negative tests for invalid inputs and ownership errors.

Exit condition:

- Public API behavior is documented and tested.

## Phase 4: CI and Release Automation

- Add Linux, macOS, and Windows CI.
- Add fmt, clippy, test, release-test, and binding jobs.
- Add fuzz build or bounded fuzz smoke to CI.
- Add release artifact generation for supported targets.

Exit condition:

- Release checks run from CI instead of local-only manual commands.

## Phase 5: Coverage and Fuzz Hardening

- Add coverage reporting for `compact-core`.
- Raise core coverage toward 90%+ with targeted tests.
- Expand fuzz targets around decode, repair, migration, and parallel paths.
- Track crash regressions as blocking release issues.

Exit condition:

- Coverage and fuzz results are visible in CI.

## Phase 6: Production Benchmarks

- Create reproducible benchmark fixtures or fixture generators.
- Benchmark log archive, search index, streaming snapshot, and analytics
  workloads.
- Add performance regression thresholds where stable enough.
- Publish benchmark commands and results.

Exit condition:

- v1.0 release notes can cite reproducible production benchmark results.

## Phase 7: Final Release Gate

- Run the complete release checklist.
- Verify no active `REVIEWER` comments remain except policy docs.
- Verify no known crash bugs remain.
- Verify docs, tags, and crate versions agree.
- Tag and push v1.0.0.

Exit condition:

- `v1.0.0` is tagged from a clean working tree after CI passes.
