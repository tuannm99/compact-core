const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const compact = require("./index.js");

const input = Buffer.from("aaaaabbbbbbbbbcccccccc");
const encoded = compact.encodeBytesRle(input);
const decoded = compact.decodeBytesRle(encoded);

assert.deepEqual(decoded, input);
assert.ok(compact.version().includes("compact"));

const dir = fs.mkdtempSync(path.join(os.tmpdir(), "compact-node-test-"));
try {
  const inputPath = path.join(dir, "input.jsonl");
  const schemaPath = path.join(dir, "schema.yml");
  const encodedPath = path.join(dir, "encoded.cmp");
  const decodedPath = path.join(dir, "decoded.jsonl");
  fs.writeFileSync(inputPath, '{"ts":1}\n');
  fs.writeFileSync(
    schemaPath,
    "columns:\n  - name: ts\n    type: u64\n    codec: delta_varint_u64\n",
  );
  compact.encodeFile(inputPath, schemaPath, encodedPath);
  compact.decodeFile(encodedPath, schemaPath, decodedPath);
  assert.deepEqual(fs.readFileSync(decodedPath), fs.readFileSync(inputPath));
  assert.throws(() => compact.decodeFile(Buffer.from("bad"), schemaPath, decodedPath));
} finally {
  fs.rmSync(dir, { recursive: true, force: true });
}
