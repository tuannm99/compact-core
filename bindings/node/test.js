const assert = require("node:assert/strict");
const compact = require("./index.js");

const input = Buffer.from("aaaaabbbbbbbbbcccccccc");
const encoded = compact.encodeBytesRle(input);
const decoded = compact.decodeBytesRle(encoded);

assert.deepEqual(decoded, input);
assert.ok(compact.version().includes("compact"));
