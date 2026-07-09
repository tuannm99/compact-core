# v1.0 Scope and Decisions

## Goal

Ship `compact-core` as a stable, portable compression platform with production
release discipline.

## In Scope

- Publish a complete CMP1-CMP4 format specification.
- Freeze the stable Rust API surface that downstream users may rely on.
- Freeze the stable C ABI and binding behavior for Go, Python, and Node.
- Define compatibility rules for future patch and minor releases.
- Add release CI for Linux, macOS, and Windows.
- Integrate fuzzing into CI for release-critical decode and repair paths.
- Add coverage reporting and target 90%+ coverage for `compact-core`.
- Publish repeatable production benchmarks.
- Add performance regression tests for important encode/decode paths.
- Validate real workloads:
  - log archive
  - search index
  - streaming snapshot
  - analytics workload

## Out of Scope

- New CMP5 format work unless CMP1-CMP4 cannot safely satisfy a stable contract.
- Large rewrites of working subsystems without a concrete safety or correctness
  defect.
- Unbounded feature expansion in SDKs before API stability is reviewed.
- Benchmark claims that cannot be reproduced from documented commands.

## Stability Policy

After v1.0, users should be able to rely on:

- documented file formats remaining readable;
- documented public APIs keeping source compatibility within a major version;
- FFI ownership rules remaining stable;
- CLI commands preserving documented input/output behavior;
- repair and migration tools refusing unsafe output paths.

Any future breaking change must be documented as a major-version decision.

## Release Gate

Do not tag v1.0 until all Definition of Done items in
`docs/definition-of-done.md` are either complete or explicitly documented as
non-goals with a reason.
