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

SMOKE_DIR="$(mktemp -d /tmp/compact-cross-language.XXXXXX)"
trap 'rm -rf "$SMOKE_DIR"' EXIT
printf '{"ts":1}\n{"ts":2}\n' >"$SMOKE_DIR/input.jsonl"
printf 'columns:\n  - name: ts\n    type: u64\n    codec: delta_varint_u64\n' >"$SMOKE_DIR/schema.yml"

PYTHONPATH="$ROOT/bindings/python" python3 -c \
  'import compact,sys; compact.encode_file(sys.argv[1],sys.argv[2],sys.argv[3])' \
  "$SMOKE_DIR/input.jsonl" "$SMOKE_DIR/schema.yml" "$SMOKE_DIR/python.cmp"
node -e \
  'const c=require(process.argv[1]); c.decodeFile(process.argv[2],process.argv[3],process.argv[4]); c.encodeFile(process.argv[4],process.argv[3],process.argv[5])' \
  "$ROOT/bindings/node" "$SMOKE_DIR/python.cmp" "$SMOKE_DIR/schema.yml" \
  "$SMOKE_DIR/node-decoded.jsonl" "$SMOKE_DIR/node.cmp"
cmp "$SMOKE_DIR/input.jsonl" "$SMOKE_DIR/node-decoded.jsonl"

export COMPACT_CROSS_INPUT="$SMOKE_DIR/input.jsonl"
export COMPACT_CROSS_SCHEMA="$SMOKE_DIR/schema.yml"
export COMPACT_CROSS_ENCODED="$SMOKE_DIR/node.cmp"
export COMPACT_CROSS_GO_ENCODED="$SMOKE_DIR/go.cmp"
(cd bindings/go && go test ./...)
PYTHONPATH="$ROOT/bindings/python" python3 -c \
  'import compact,sys; compact.decode_file(sys.argv[1],sys.argv[2],sys.argv[3])' \
  "$SMOKE_DIR/go.cmp" "$SMOKE_DIR/schema.yml" "$SMOKE_DIR/python-decoded.jsonl"
cmp "$SMOKE_DIR/input.jsonl" "$SMOKE_DIR/python-decoded.jsonl"

PYTHONPATH="$ROOT/bindings/python" python3 -m unittest discover -s bindings/python -p 'test_*.py'
npm test --prefix bindings/node
