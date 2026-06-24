#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

cargo build -p compact-cli -p compact-ffi

export COMPACT_BIN="$ROOT/target/debug/compact"
export COMPACT_FFI_LIB="$ROOT/target/debug/libcompact_ffi.so"
export LD_LIBRARY_PATH="$ROOT/target/debug${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export CGO_LDFLAGS="-L$ROOT/target/debug -lcompact_ffi"
export GOCACHE="${GOCACHE:-/tmp/compact-core-go-build}"

(cd bindings/go && go test ./...)
PYTHONPATH="$ROOT/bindings/python" python3 -m unittest discover -s bindings/python -p 'test_*.py'
npm test --prefix bindings/node
