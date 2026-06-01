# Architecture

- `crates/compact-core`: compression primitives, codecs, frame format, schema, readers/writers
- `crates/compact-cli`: command-line entrypoint for encode/decode/inspect/bench
- `crates/compact-ffi`: stable C ABI for other languages
- `bindings/go`: first consumer binding over the C ABI
- `fuzz`: libFuzzer targets for decode safety, documented in [fuzzing.md](fuzzing.md)

The v0.1 support contract is documented in [v0.1.md](v0.1.md).

Implementation order follows the milestone list:

1. primitives
2. codecs
3. frame format
4. schema + column blocks
5. CLI
6. benchmark + fuzzing
7. streaming
8. FFI + Go binding
