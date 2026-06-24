const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

function compactBin() {
  return process.env.COMPACT_BIN || "compact";
}

function encodeFile(input, schema, output) {
  runCompact(["encode", input, output, "--schema", schema]);
}

function decodeFile(input, schema, output) {
  runCompact(["decode", input, output, "--schema", schema]);
}

function version() {
  return runCompact(["--version"]).stdout.toString("utf8").trim();
}

function encodeBytesRle(input) {
  return withTempFiles(input, (inputPath, outputPath) => {
    runCompact(["encode", inputPath, outputPath]);
    return fs.readFileSync(outputPath);
  });
}

function decodeBytesRle(input) {
  return withTempFiles(input, (inputPath, outputPath) => {
    runCompact(["decode", inputPath, outputPath]);
    return fs.readFileSync(outputPath);
  });
}

function runCompact(args) {
  const result = spawnSync(compactBin(), args, { stdio: "pipe" });
  if (result.status === 0) {
    return result;
  }

  const stderr = result.stderr?.toString("utf8") || "";
  const message = result.error ? `${result.error.message}\n${stderr}` : stderr;
  throw new Error(message.trim() || `compact exited with status ${result.status}`);
}

function withTempFiles(input, callback) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "compact-node-"));
  const inputPath = path.join(dir, "input.bin");
  const outputPath = path.join(dir, "output.bin");

  try {
    fs.writeFileSync(inputPath, Buffer.from(input));
    return callback(inputPath, outputPath);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

module.exports = {
  decodeBytesRle,
  decodeFile,
  encodeBytesRle,
  encodeFile,
  version,
};
