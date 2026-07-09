# Phase 1 Audit Report

Date: 2026-07-10

Baseline:

- Branch: `master`
- Current release tag: `v0.9.1`
- Current head after release tag: v1.0 planning docs
- Ignored local file: `restore-codex`

## Verdict

Phase 1 is in progress. The project has enough implemented surface to begin
v1.0 stabilization, but the public contract is too broad to freeze as-is.

The main risk is not missing functionality. The main risk is accidentally
stabilizing internal modules, experimental CLI commands, and format details
before they are specified.

## Blocking Items for v1.0

These must be resolved before tagging v1.0.0.

- Cargo package version is still `0.9.0` while the latest release tag is
  `v0.9.1`. The release process must keep crate metadata, CLI `--version`, FFI
  `compact_version`, docs, and tags aligned.
- CMP1-CMP4 do not yet have one complete public format specification that
  documents byte layout, limits, checksums, compatibility behavior, and recovery
  behavior.
- Public Rust module surface is broader than the likely stable API. Modules
  such as low-level `format`, `codecs`, `pipeline`, `statistics`, and selected
  primitives need explicit stable/internal/experimental classification.
- CLI commands do not yet have a single stable command contract covering input,
  output, overwrite behavior, atomicity, exit-code behavior, and security
  constraints.
- CI/release automation is not yet documented as satisfying Linux, macOS, and
  Windows build/test requirements.
- Coverage reporting is not yet wired into the release gate, so the 90%+
  `compact-core` target cannot be claimed.
- Fuzzing exists locally, but release-critical fuzz targets still need CI
  integration or a documented bounded fuzz smoke gate.
- Production workload benchmarks are not complete for log archive, search
  index, streaming snapshot, and analytics workload.

## Non-Blocking Suggestions

These improve maintainability but do not need to block early v1.0 audit work.

- Keep Node as a CLI-backed SDK until there is a clear need for native FFI.
- Keep experimental codec enum variants documented as unsupported instead of
  removing them immediately.
- Prefer a small stable Rust prelude or facade instead of promising every
  public module forever.
- Record benchmark results in docs only when commands, fixture generation, and
  hardware metadata are included.

## Rust API Inventory

Recommended classification:

| Surface | Current status | v1.0 classification | Notes |
| --- | --- | --- | --- |
| `crate_version` | public | stable | Must match Cargo version and release tag. |
| `checksum32` | public | stable | Simple facade over CRC32. |
| `CompactError` | public | stable | Needs compatibility policy for variants. |
| `EncodeConfig`, `ValueType`, `Transform`, `Codec` | public | stable with caveats | Unsupported enum variants must remain explicitly documented. |
| `encode_bytes_frame`, `decode_bytes_frame` | public | stable | CMP1 raw-byte API. |
| `encode_u64_frame`, `decode_u64_frame` | public | stable | CMP1 numeric API. |
| `primitives::varint`, `zigzag`, `delta`, `rle`, `bitpack`, `bitmap`, `crc32` | public | stable or experimental per module | Low-level APIs need docs for malformed input and limits. |
| `framing` | public | stable if CMP1 is public | Must be covered by format spec. |
| `streaming` | public | stable | CMP2 APIs are central and should be documented. |
| `schema` and `schema::evolution` | public | stable | Needed for storage compatibility. |
| `storage` | public | stable | Needed for validation, repair, and migration. |
| `io::v3`, `io::v4` | public | stable | Format-specific APIs should be documented as such. |
| `format::v3`, `format::v4` | public | experimental/internal candidate | Low-level layout builders may be too brittle for stable users. |
| `codecs::v3` | public | experimental/internal candidate | Better exposed through `io::v3` unless users need column chunks. |
| `parallel` | public | stable after CI/perf gates | Needs benchmark and regression policy. |
| `search` | public | stable after workload validation | Needs search-index workload benchmark. |
| `statistics` | public | experimental/internal candidate | Depends on CMP4 metadata spec. |
| `pipeline` | public | experimental/internal candidate | Currently lower-level than the stable facade. |

## CLI Inventory

Commands currently exposed:

- `encode`
- `decode`
- `inspect`
- `validate`
- `schema-check`
- `evolve-decode`
- `repair`
- `metadata-migrate`
- `repair-bench`
- `bench`
- `parallel-bench`
- `search-encode`
- `search-inspect`
- `search-lookup`
- `search-seek`
- `search-bench`
- `stream-append`
- `stream-recover`
- `stream-replay`
- `stream-roll`
- `stream-bench`
- `snapshot-encode`
- `snapshot-decode`

Recommended v1.0 stable CLI groups:

- Core file commands: `encode`, `decode`, `inspect`, `validate`
- Storage safety commands: `repair`, `metadata-migrate`, `schema-check`,
  `evolve-decode`
- Streaming commands: `stream-append`, `stream-recover`, `stream-replay`,
  `stream-roll`
- Search commands: `search-encode`, `search-inspect`, `search-lookup`,
  `search-seek`
- Benchmark commands: stable enough for reproducible reports but not for
  machine-parsed output compatibility unless explicitly documented
- Snapshot commands: stable only after snapshot workload validation

The CLI contract still needs a dedicated reference document before v1.0.

## FFI and Binding Inventory

Stable C ABI candidates:

- `compact_version`
- `compact_status_message`
- `compact_buffer_free`
- `compact_encode_bytes_rle`
- `compact_decode_bytes_rle`
- `compact_encode_file`
- `compact_decode_file`
- `compact_decode_file_with_schema`

Binding status:

- Go uses the C ABI directly and validates embedded NUL paths.
- Python uses `ctypes` and copies Rust-owned output buffers before freeing.
- Node currently shells out to the CLI instead of using the C ABI.

v1.0 requirement:

- C ABI function names, statuses, and memory ownership rules need a stable ABI
  reference.
- Binding behavior should explicitly state whether each binding is native ABI or
  CLI-backed.
- File roundtrip compatibility should remain part of the cross-language smoke
  suite.

## Format Inventory

Formats currently in scope:

- CMP1: framed byte/numeric codec format
- CMP2: streaming JSONL block format
- CMP3: typed column layout
- CMP4: queryable columnar layout with footer/index

Spec gaps to close:

- exact byte layout for every header, block, metadata record, footer, and
  trailer;
- endian rules;
- varint rules;
- checksum/digest coverage;
- maximum counts and allocation limits;
- unknown version behavior;
- forward/backward compatibility behavior;
- corruption and partial-file recovery behavior.

## Immediate Next Work

1. Sync Cargo package version with the latest release line or document why the
   tag is patch-only without a crate version bump.
2. Write `docs/format-spec.md` covering CMP1-CMP4.
3. Write `docs/api-stability.md` classifying Rust, FFI, SDK, and CLI surfaces.
4. Add a CI/release checklist document that maps directly to the v1.0 DoD.
5. Add coverage and fuzz gates to CI.

## Local Quality Gate

Executed on 2026-07-10:

```text
cargo fmt --all -- --check
git diff --check
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features --release
scripts/cross_language_smoke.sh
```

Result:

- All commands passed.
- Workspace tests passed in debug and release mode.
- Cross-language smoke passed for Go, Python, and Node.
- Build output still identifies crates as `v0.9.0`, confirming the release
  metadata blocker above.
