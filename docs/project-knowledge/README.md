# Project Knowledge

This folder separates general compression knowledge from implementation-specific
knowledge for `compact-core`.

## Concepts

General knowledge that applies beyond this repository:

- [Foundation](concepts/01-foundation.md)
- [Compression math](concepts/02-compression-math.md)
- [Compression primitives](concepts/03-primitives.md)
- [Compression algorithms](concepts/04-algorithms.md)
- [Systems engineering](concepts/05-systems-engineering.md)
- [Rust and performance](concepts/06-rust-performance.md)
- [Testing and benchmarking](concepts/07-testing-benchmarking.md)

## compact-core Implementation

How those concepts are applied in this project:

- [Architecture](compact-core/01-architecture.md)
- [Build roadmap](compact-core/02-build-roadmap.md)
- [v0.2 streaming index](compact-core/03-v0.2-streaming-index.md)
- [Rules and reminders](compact-core/04-rules-and-reminders.md)
- [v0.2 streaming detail](compact-core/streaming/README.md)
- [v0.3 advanced column compression](compact-core/v0.3/README.md)
- [v0.4 queryable columnar format](compact-core/v0.4/README.md)
- [v0.5 search engine compression](compact-core/v0.5/README.md)
- [v0.6 real-time streaming integration](compact-core/v0.6/README.md)
- [v0.7 FFI and multi-language SDK](compact-core/v0.7/README.md)

Use the concept files to learn the underlying ideas. Use the `compact-core`
files when deciding what to build in this repository.
