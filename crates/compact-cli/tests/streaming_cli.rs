use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn compact() -> &'static str {
    env!("CARGO_BIN_EXE_compact")
}

fn temp_case(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("compact-cli-{name}-{}-{nonce}", std::process::id()));

    fs::create_dir_all(&dir).expect("test temp dir should be created");

    dir
}

fn write_fixture(dir: &Path) -> (PathBuf, PathBuf) {
    let input = dir.join("input.jsonl");
    let schema = dir.join("schema.yml");

    fs::write(
        &input,
        concat!(
            "{\"ts\":100,\"level\":\"INFO\"}\n",
            "{\"ts\":101,\"level\":\"WARN\"}\n",
            "{\"ts\":102,\"level\":\"INFO\"}\n",
        ),
    )
    .expect("input fixture should be written");
    fs::write(
        &schema,
        concat!(
            "columns:\n",
            "  - name: ts\n",
            "    type: u64\n",
            "    codec: delta_varint_u64\n",
            "  - name: level\n",
            "    type: string\n",
            "    codec: rle\n",
        ),
    )
    .expect("schema fixture should be written");

    (input, schema)
}

fn write_generated_fixture(dir: &Path, rows: usize) -> (PathBuf, PathBuf) {
    let input = dir.join("generated.jsonl");
    let schema = dir.join("schema.yml");
    let mut jsonl = String::new();

    for index in 0..rows {
        let level = match index % 3 {
            0 => "INFO",
            1 => "WARN",
            _ => "ERROR",
        };
        jsonl.push_str(&format!(
            "{{\"ts\":{},\"level\":\"{}\"}}\n",
            1_700_000_000u64 + index as u64,
            level
        ));
    }

    fs::write(&input, jsonl).expect("generated input fixture should be written");
    fs::write(
        &schema,
        concat!(
            "columns:\n",
            "  - name: ts\n",
            "    type: u64\n",
            "    codec: delta_varint_u64\n",
            "  - name: level\n",
            "    type: string\n",
            "    codec: rle\n",
        ),
    )
    .expect("schema fixture should be written");

    (input, schema)
}

fn run(args: &[&str]) -> Output {
    Command::new(compact())
        .args(args)
        .output()
        .expect("compact command should run")
}

fn write_v3_fixture(dir: &Path) -> (PathBuf, PathBuf) {
    let input = dir.join("v3.jsonl");
    let schema = dir.join("v3.yml");
    fs::write(
        &input,
        "{\"ts\":1000,\"active\":true,\"path\":\"service/api\"}\n{\"ts\":1001,\"active\":false,\"path\":\"service/admin\"}\n",
    )
    .unwrap();
    fs::write(
        &schema,
        "columns:\n  - name: ts\n    type: u64\n    codec: auto\n  - name: active\n    type: bool\n    codec: auto\n  - name: path\n    type: string\n    codec: auto\n",
    )
    .unwrap();
    (input, schema)
}

fn write_v4_fixture(dir: &Path) -> (PathBuf, PathBuf) {
    let input = dir.join("v4.jsonl");
    let schema = dir.join("v4.yml");
    fs::write(
        &input,
        concat!(
            "{\"id\":1,\"active\":true,\"service\":\"api\"}\n",
            "{\"id\":2,\"active\":false,\"service\":\"api\"}\n",
            "{\"id\":10,\"active\":true,\"service\":null}\n",
            "{\"id\":11,\"active\":false,\"service\":\"worker\"}\n",
        ),
    )
    .unwrap();
    fs::write(
        &schema,
        concat!(
            "columns:\n",
            "  - name: id\n",
            "    type: u64\n",
            "    codec: delta_bitpack\n",
            "  - name: active\n",
            "    type: bool\n",
            "    codec: bitmap\n",
            "  - name: service\n",
            "    type: string\n",
            "    codec: prefix\n",
            "    nullable: true\n",
        ),
    )
    .unwrap();
    (input, schema)
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "expected success\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_failure(output: &Output) {
    assert!(
        !output.status.success(),
        "expected failure\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn schema_encode_decode_roundtrips_streaming_jsonl() {
    let dir = temp_case("roundtrip");
    let (input, schema) = write_fixture(&dir);
    let encoded = dir.join("encoded.cmp");
    let decoded = dir.join("decoded.jsonl");

    let encode = run(&[
        "encode",
        input.to_str().unwrap(),
        encoded.to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
        "--block-rows",
        "2",
    ]);
    assert_success(&encode);

    let decode = run(&[
        "decode",
        encoded.to_str().unwrap(),
        decoded.to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
    ]);
    assert_success(&decode);

    assert_eq!(
        fs::read_to_string(&decoded).unwrap(),
        fs::read_to_string(&input).unwrap()
    );

    fs::remove_dir_all(dir).ok();
}

#[test]
fn inspect_reports_streaming_block_metadata() {
    let dir = temp_case("inspect");
    let (input, schema) = write_fixture(&dir);
    let encoded = dir.join("encoded.cmp");

    let encode = run(&[
        "encode",
        input.to_str().unwrap(),
        encoded.to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
        "--block-rows",
        "2",
    ]);
    assert_success(&encode);

    let inspect = run(&["inspect", encoded.to_str().unwrap()]);
    assert_success(&inspect);
    let stdout = String::from_utf8_lossy(&inspect.stdout);

    assert!(stdout.contains("version: 2"));
    assert!(stdout.contains("format: stream"));
    assert!(stdout.contains("blocks: 2"));
    assert!(stdout.contains("index: footer"));
    assert!(stdout.contains("index_blocks: 2"));
    assert!(stdout.contains("total_rows: 3"));
    assert!(stdout.contains("block 0 offset=10 rows=2"));
    assert!(stdout.contains("block 1 offset="));

    fs::remove_dir_all(dir).ok();
}

#[test]
fn bench_reports_streaming_metrics() {
    let dir = temp_case("bench");
    let (input, schema) = write_fixture(&dir);

    let bench = run(&[
        "bench",
        input.to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
        "--block-rows",
        "2",
    ]);
    assert_success(&bench);
    let stdout = String::from_utf8_lossy(&bench.stdout);

    assert!(stdout.contains("mode: stream"));
    assert!(stdout.contains("block_rows: 2"));
    assert!(stdout.contains("blocks: 2"));
    assert!(stdout.contains("rows: 3"));
    assert!(stdout.contains("encode_mib_s:"));
    assert!(stdout.contains("decode_mib_s:"));

    fs::remove_dir_all(dir).ok();
}

#[test]
fn invalid_block_options_fail_before_writing_success() {
    let dir = temp_case("invalid-options");
    let (input, schema) = write_fixture(&dir);
    let encoded = dir.join("encoded.cmp");

    let output = run(&[
        "encode",
        input.to_str().unwrap(),
        encoded.to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
        "--block-rows",
        "0",
    ]);
    assert_failure(&output);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(stderr.contains("invalid streaming block options"));
    assert!(stderr.contains("max rows per block must be greater than zero"));

    fs::remove_dir_all(dir).ok();
}

#[test]
fn generated_jsonl_roundtrips_across_many_blocks() {
    let dir = temp_case("generated-large");
    let rows = 10_000usize;
    let rows_per_block = 1_000usize;
    let (input, schema) = write_generated_fixture(&dir, rows);
    let encoded = dir.join("generated.cmp");
    let decoded = dir.join("generated.decoded.jsonl");

    let encode = run(&[
        "encode",
        input.to_str().unwrap(),
        encoded.to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
        "--block-rows",
        &rows_per_block.to_string(),
    ]);
    assert_success(&encode);

    let inspect = run(&["inspect", encoded.to_str().unwrap()]);
    assert_success(&inspect);
    let stdout = String::from_utf8_lossy(&inspect.stdout);

    assert!(stdout.contains("format: stream"));
    assert!(stdout.contains("blocks: 10"));
    assert!(stdout.contains("total_rows: 10000"));

    let decode = run(&[
        "decode",
        encoded.to_str().unwrap(),
        decoded.to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
    ]);
    assert_success(&decode);

    assert_eq!(fs::read(&decoded).unwrap(), fs::read(&input).unwrap());

    fs::remove_dir_all(dir).ok();
}

#[test]
fn cmp3_cli_roundtrip_inspect_and_bench() {
    let dir = temp_case("cmp3");
    let (input, schema) = write_v3_fixture(&dir);
    let encoded = dir.join("encoded.cmp3");
    let decoded = dir.join("decoded.jsonl");

    assert_success(&run(&[
        "encode",
        input.to_str().unwrap(),
        encoded.to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
        "--format",
        "v3",
    ]));
    assert_success(&run(&[
        "decode",
        encoded.to_str().unwrap(),
        decoded.to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
        "--format",
        "v3",
    ]));
    assert_eq!(fs::read(&decoded).unwrap(), fs::read(&input).unwrap());

    let inspect = run(&["inspect", encoded.to_str().unwrap()]);
    assert_success(&inspect);
    let stdout = String::from_utf8_lossy(&inspect.stdout);
    assert!(stdout.contains("format: cmp3"));
    assert!(stdout.contains("codec="));
    assert!(stdout.contains("stats="));

    let bench = run(&[
        "bench",
        input.to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
        "--format",
        "v3",
    ]);
    assert_success(&bench);
    assert!(String::from_utf8_lossy(&bench.stdout).contains("mode: v3"));

    fs::remove_dir_all(dir).ok();
}

#[test]
fn cmp4_cli_roundtrip_inspect_projection_filter_and_bench() {
    let dir = temp_case("cmp4");
    let (input, schema) = write_v4_fixture(&dir);
    let encoded = dir.join("encoded.cmp4");
    let decoded = dir.join("decoded.jsonl");
    let projected = dir.join("projected.jsonl");
    let filtered = dir.join("filtered.jsonl");

    assert_success(&run(&[
        "encode",
        input.to_str().unwrap(),
        encoded.to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
        "--format",
        "v4",
        "--block-rows",
        "2",
    ]));
    assert_success(&run(&[
        "decode",
        encoded.to_str().unwrap(),
        decoded.to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
        "--format",
        "v4",
    ]));
    assert_eq!(fs::read(&decoded).unwrap(), fs::read(&input).unwrap());

    let inspect = run(&["inspect", encoded.to_str().unwrap()]);
    assert_success(&inspect);
    let stdout = String::from_utf8_lossy(&inspect.stdout);
    assert!(stdout.contains("format: cmp4"));
    assert!(stdout.contains("row_groups: 2"));
    assert!(stdout.contains("rows: 4"));
    assert!(stdout.contains("payload_offset="));
    assert!(stdout.contains("stats="));

    assert_success(&run(&[
        "decode",
        encoded.to_str().unwrap(),
        projected.to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
        "--format",
        "v4",
        "--project",
        "id,service",
    ]));
    assert_eq!(
        fs::read_to_string(&projected).unwrap(),
        concat!(
            "{\"id\":1,\"service\":\"api\"}\n",
            "{\"id\":2,\"service\":\"api\"}\n",
            "{\"id\":10,\"service\":null}\n",
            "{\"id\":11,\"service\":\"worker\"}\n",
        )
    );

    assert_success(&run(&[
        "decode",
        encoded.to_str().unwrap(),
        filtered.to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
        "--format",
        "v4",
        "--project",
        "id",
        "--filter-column",
        "id",
        "--filter-op",
        "ge",
        "--filter-value",
        "10",
    ]));
    assert_eq!(
        fs::read_to_string(&filtered).unwrap(),
        "{\"id\":10}\n{\"id\":11}\n"
    );

    let bench = run(&[
        "bench",
        input.to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
        "--format",
        "v4",
        "--block-rows",
        "2",
    ]);
    assert_success(&bench);
    let stdout = String::from_utf8_lossy(&bench.stdout);
    assert!(stdout.contains("mode: v4"));
    assert!(stdout.contains("row_groups: 2"));
    assert!(stdout.contains("projected_decode_ms:"));

    fs::remove_dir_all(dir).ok();
}
