# Fuzzing

Fuzzing is used to stress decoders with malformed input.

The v0.1 fuzz target is:

```text
fuzz/fuzz_targets/frame_decode.rs
```

It feeds arbitrary bytes into:

```rust
compact_core::framing::decode_v1(data)
```

The queryable-format targets are:

```text
fuzz/fuzz_targets/cmp3_decode.rs
fuzz/fuzz_targets/cmp4_decode.rs
fuzz/fuzz_targets/search_decode.rs
```

`cmp4_decode` feeds arbitrary bytes into footer inspect, full decode,
projection decode, and predicate scan paths. The expected behavior is always
safe return with `Ok` or `Err`; malformed bytes must not panic.

`search_decode` feeds arbitrary bytes into PST1 posting-list decode, TRM1
dictionary decode, term lookup, docID seek, and query helper paths. Search
payloads are untrusted file bytes too, so malformed data must never panic.

The expected behavior is not successful decoding. The expected behavior is that
the decoder always returns safely with `Ok` or `Err`.

## Why It Exists

Frame decoding receives untrusted bytes from files and language bindings.

The fuzz target helps catch:

- panics
- slice out-of-bounds bugs
- integer overflow around payload lengths
- malformed header handling bugs
- checksum and codec-id validation bugs
- regressions where corrupted files crash instead of returning an error

This maps directly to the v0.1 Definition of Done:

- fuzz test exists for frame decoder
- corrupted file never panics
- invalid frame handled safely

## Build Check

CI runs this command to verify that the fuzz target compiles:

```sh
cargo test --manifest-path fuzz/Cargo.toml
```

This does not run long fuzzing. It is a fast build-level guard.

## Running Fuzzing Locally

Install `cargo-fuzz`:

```sh
cargo install cargo-fuzz
```

Run the frame decoder fuzz target:

```sh
cargo fuzz run frame_decode
cargo fuzz run cmp3_decode
cargo fuzz run cmp4_decode
cargo fuzz run search_decode
```

Let it run for several minutes during normal development. Let it run longer
before changing frame parsing logic or release-critical decode paths.

## Current Target

```rust
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = compact_core::framing::decode_v1(data);
});
```

Ignoring the return value is intentional. The safety property is that every
input is handled without crashing.
