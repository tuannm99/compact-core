# Stabilization Audit

This audit is phase 1 for v1.0. Its job is to find unstable contracts before
they become public promises.

## Format Audit

Review every documented and implemented format:

- CMP1 frame layout
- CMP2 streaming block layout
- CMP3 typed column layout
- CMP4 footer and index layout

For each format, record:

- magic bytes and version fields;
- required section ordering;
- integer encoding and endianness;
- checksum or digest coverage;
- maximum accepted counts and sizes;
- recovery behavior for partial files;
- unknown-field and future-version behavior;
- compatibility tests that prove the contract.

The result should become the public format specification.

## Rust API Audit

List exported modules and decide whether each is stable, internal, or
experimental:

- `primitives`
- `codecs`
- `format`
- `io`
- `schema`
- `streaming`
- `search`
- `storage`
- `parallel`

For each stable API, require:

- docs that explain ownership, errors, and limits;
- tests for success and failure paths;
- no panic for malformed user input;
- clear error variants instead of string-only behavior where practical.

## FFI and Bindings Audit

Check the C ABI and language bindings for:

- stable exported function names;
- explicit memory ownership rules;
- null pointer behavior;
- embedded NUL path behavior;
- panic containment;
- cross-language file compatibility;
- version/status mapping consistency.

The ABI should stay small. Do not expose low-level internals unless there is a
real downstream use case.

## CLI Audit

For each command, document:

- input format;
- output format;
- exit-code behavior;
- whether writes are atomic;
- whether existing files are overwritten or rejected;
- security constraints for repair and migration outputs.

CLI behavior becomes part of the production contract once documented.

## Test and Automation Audit

Confirm that CI covers:

- `cargo fmt`
- `cargo clippy`
- workspace tests
- release-mode tests
- fuzz build or bounded fuzz smoke
- Go binding tests
- Python binding tests
- Node binding tests
- cross-language smoke test
- Linux, macOS, and Windows builds

Coverage reporting should identify untested core modules before the 90% target
is claimed.

## Benchmark Audit

Benchmarks must be reproducible from checked-in docs. Each benchmark report
should include:

- commit SHA;
- hardware and OS;
- command;
- dataset description;
- encode throughput;
- decode throughput;
- compression ratio;
- peak memory when available;
- comparison baseline where relevant.

Avoid publishing performance claims from debug builds.
