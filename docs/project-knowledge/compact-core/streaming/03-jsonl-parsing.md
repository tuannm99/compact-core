# Streaming JSONL Parsing

JSONL is line-oriented:

```text
{"ts":100,"level":"INFO"}
{"ts":101,"level":"WARN"}
```

Each row is one line.

The streaming parser should use `BufRead`:

```rust
let mut line = String::new();
reader.read_line(&mut line)?;
```

Important details:

- `read_line` keeps the trailing newline.
- Empty lines should follow the v0.1 behavior. Current code skips empty lines.
- Invalid UTF-8 should return an error.
- Invalid JSON should return an error with row context when possible.
- A final line without trailing newline should still be accepted.

Avoid this for large files:

```rust
std::fs::read_to_string(path)
```

Because it loads the whole file.

Prefer this:

```rust
let file = File::open(path)?;
let reader = BufReader::new(file);
```

The first streaming implementation can parse one line at a time and push values
into the current row group.
