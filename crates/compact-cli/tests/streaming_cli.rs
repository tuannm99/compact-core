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
fn validate_reports_valid_file_and_rejects_corruption() {
    let dir = temp_case("validate");
    let (input, schema) = write_v4_fixture(&dir);
    let encoded = dir.join("encoded.cmp");

    let encode = run(&[
        "encode",
        input.to_str().unwrap(),
        encoded.to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
        "--format",
        "v4",
        "--block-rows",
        "2",
    ]);
    assert_success(&encode);

    let validate = run(&["validate", encoded.to_str().unwrap()]);
    assert_success(&validate);
    let stdout = String::from_utf8_lossy(&validate.stdout);
    assert!(stdout.contains("valid: true"));
    assert!(stdout.contains("format: CMP4"));
    assert!(stdout.contains("storage_units: 2"));
    assert!(stdout.contains("rows: 4"));

    let mut bytes = fs::read(&encoded).unwrap();
    bytes[32] ^= 0xff;
    fs::write(&encoded, bytes).unwrap();
    let corrupted = run(&["validate", encoded.to_str().unwrap()]);
    assert_failure(&corrupted);
    assert!(
        String::from_utf8_lossy(&corrupted.stderr).contains("cmp4 row group checksum mismatch")
    );

    fs::remove_dir_all(dir).ok();
}

#[test]
fn schema_check_and_evolved_decode_apply_compatible_revision() {
    let dir = temp_case("schema-evolution");
    let (input, schema) = write_v4_fixture(&dir);
    let encoded = dir.join("encoded.cmp");
    let decoded = dir.join("evolved.jsonl");
    let writer_revision = dir.join("writer-revision.yml");
    let reader_revision = dir.join("reader-revision.yml");
    fs::write(
        &writer_revision,
        concat!(
            "revision: 1\n",
            "columns:\n",
            "  - {stable_id: 1, name: id, type: u64, codec: delta_bitpack}\n",
            "  - {stable_id: 2, name: active, type: bool, codec: bitmap}\n",
            "  - {stable_id: 3, name: service, type: string, codec: prefix, nullable: true}\n",
        ),
    )
    .unwrap();
    fs::write(
        &reader_revision,
        concat!(
            "revision: 2\n",
            "columns:\n",
            "  - {stable_id: 1, name: event_id, type: u64, codec: bitpack, aliases: [id]}\n",
            "  - {stable_id: 4, name: region, type: string, codec: stored, nullable: true}\n",
            "  - {stable_id: 5, name: source, type: string, codec: stored, default: compact}\n",
        ),
    )
    .unwrap();

    assert_success(&run(&[
        "encode",
        input.to_str().unwrap(),
        encoded.to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
        "--format",
        "v4",
    ]));

    let check = run(&[
        "schema-check",
        writer_revision.to_str().unwrap(),
        reader_revision.to_str().unwrap(),
    ]);
    assert_success(&check);
    assert!(String::from_utf8_lossy(&check.stdout).contains("compatible: true"));

    let evolve = run(&[
        "evolve-decode",
        encoded.to_str().unwrap(),
        decoded.to_str().unwrap(),
        "--writer-schema",
        writer_revision.to_str().unwrap(),
        "--reader-schema",
        reader_revision.to_str().unwrap(),
    ]);
    assert_success(&evolve);
    let rows = fs::read_to_string(decoded).unwrap();
    assert!(rows.contains(r#""event_id":1"#));
    assert!(rows.contains(r#""region":null"#));
    assert!(rows.contains(r#""source":"compact""#));
    assert!(!rows.contains("service"));

    fs::remove_dir_all(dir).ok();
}

#[test]
fn repair_dry_run_and_copy_on_write_recover_cmp2_prefix() {
    let dir = temp_case("repair");
    let (input, schema) = write_fixture(&dir);
    let append = dir.join("append.cmp");
    let repaired = dir.join("repaired.cmp");

    assert_success(&run(&[
        "stream-append",
        input.to_str().unwrap(),
        append.to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
        "--block-rows",
        "1",
    ]));
    let mut bytes = fs::read(&append).unwrap();
    *bytes.last_mut().unwrap() ^= 0xff;
    fs::write(&append, bytes).unwrap();

    let dry_run = run(&["repair", append.to_str().unwrap(), "--dry-run"]);
    assert_success(&dry_run);
    let stdout = String::from_utf8_lossy(&dry_run.stdout);
    assert!(stdout.contains("action: TruncateTailAndRebuildFooter"));
    assert!(stdout.contains("recovered_units: 2"));

    let repair = run(&[
        "repair",
        append.to_str().unwrap(),
        "--output",
        repaired.to_str().unwrap(),
    ]);
    assert_success(&repair);
    assert_ne!(fs::read(&append).unwrap(), fs::read(&repaired).unwrap());

    let validate = run(&["validate", repaired.to_str().unwrap()]);
    assert_success(&validate);
    assert!(String::from_utf8_lossy(&validate.stdout).contains("footer_index: true"));

    fs::remove_dir_all(dir).ok();
}

#[cfg(unix)]
#[test]
fn repair_rejects_hard_link_and_symlink_outputs_to_source() {
    use std::os::unix::fs::symlink;

    let dir = temp_case("repair-alias");
    let (input, schema) = write_fixture(&dir);
    let source = dir.join("append.cmp");
    assert_success(&run(&[
        "stream-append",
        input.to_str().unwrap(),
        source.to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
    ]));
    let original = fs::read(&source).unwrap();

    let hard_link = dir.join("hard-link.cmp");
    fs::hard_link(&source, &hard_link).unwrap();
    assert_failure(&run(&[
        "repair",
        source.to_str().unwrap(),
        "--output",
        hard_link.to_str().unwrap(),
    ]));
    assert_eq!(fs::read(&source).unwrap(), original);
    fs::remove_file(&hard_link).unwrap();

    let symlink_path = dir.join("symlink.cmp");
    symlink(&source, &symlink_path).unwrap();
    assert_failure(&run(&[
        "repair",
        source.to_str().unwrap(),
        "--output",
        symlink_path.to_str().unwrap(),
    ]));
    assert_eq!(fs::read(&source).unwrap(), original);

    fs::remove_dir_all(dir).ok();
}

#[test]
fn repair_rebuilds_damaged_cmp4_footer() {
    let dir = temp_case("repair-cmp4");
    let (input, schema) = write_v4_fixture(&dir);
    let encoded = dir.join("damaged.cmp");
    let repaired = dir.join("repaired.cmp");

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
    let mut bytes = fs::read(&encoded).unwrap();
    *bytes.last_mut().unwrap() ^= 0xff;
    fs::write(&encoded, bytes).unwrap();

    let repair = run(&[
        "repair",
        encoded.to_str().unwrap(),
        "--output",
        repaired.to_str().unwrap(),
    ]);
    assert_success(&repair);
    let stdout = String::from_utf8_lossy(&repair.stdout);
    assert!(stdout.contains("format: CMP4"));
    assert!(stdout.contains("recovered_units: 2"));
    assert!(stdout.contains("recovered_rows: 4"));

    let validate = run(&["validate", repaired.to_str().unwrap()]);
    assert_success(&validate);
    assert!(String::from_utf8_lossy(&validate.stdout).contains("rows: 4"));

    fs::remove_dir_all(dir).ok();
}

#[test]
fn metadata_migrate_plans_writes_and_is_idempotent() {
    let dir = temp_case("metadata-migrate");
    let source = dir.join("metadata-v1.yml");
    let migrated = dir.join("metadata-v2.yml");
    let second = dir.join("metadata-v2-copy.yml");
    fs::write(
        &source,
        concat!(
            "metadata_version: 1\n",
            "revision: 1\n",
            "owner: storage-team\n",
            "columns:\n",
            "  - {name: id, type: u64, codec: delta_bitpack, role: primary_key}\n",
            "  - {name: service, type: string, codec: prefix, nullable: true}\n",
        ),
    )
    .unwrap();

    let dry_run = run(&[
        "metadata-migrate",
        source.to_str().unwrap(),
        "--column-id",
        "id=10",
        "--column-id",
        "service=20",
        "--dry-run",
    ]);
    assert_success(&dry_run);
    assert!(String::from_utf8_lossy(&dry_run.stdout).contains("action: AddStableColumnIds"));
    assert!(!migrated.exists());

    let migrate = run(&[
        "metadata-migrate",
        source.to_str().unwrap(),
        "--column-id",
        "id=10",
        "--column-id",
        "service=20",
        "--output",
        migrated.to_str().unwrap(),
    ]);
    assert_success(&migrate);
    let output = fs::read_to_string(&migrated).unwrap();
    assert!(output.contains("metadata_version: 2"));
    assert!(output.contains("owner: storage-team"));
    assert!(output.contains("role: primary_key"));

    let idempotent = run(&[
        "metadata-migrate",
        migrated.to_str().unwrap(),
        "--output",
        second.to_str().unwrap(),
    ]);
    assert_success(&idempotent);
    assert!(String::from_utf8_lossy(&idempotent.stdout).contains("action: None"));
    assert_eq!(fs::read(migrated).unwrap(), fs::read(second).unwrap());

    fs::remove_dir_all(dir).ok();
}

#[test]
fn repair_bench_reports_reproducible_metrics() {
    let dir = temp_case("repair-bench");
    let (input, schema) = write_v4_fixture(&dir);
    let encoded = dir.join("damaged.cmp");

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
    let mut bytes = fs::read(&encoded).unwrap();
    *bytes.last_mut().unwrap() ^= 0xff;
    fs::write(&encoded, bytes).unwrap();

    let benchmark = run(&[
        "repair-bench",
        encoded.to_str().unwrap(),
        "--iterations",
        "3",
    ]);
    assert_success(&benchmark);
    let stdout = String::from_utf8_lossy(&benchmark.stdout);
    assert!(stdout.contains("mode: repair"));
    assert!(stdout.contains("format: CMP4"));
    assert!(stdout.contains("iterations: 3"));
    assert!(stdout.contains("execute_mib_s:"));

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
fn parallel_bench_reports_scaling_metrics() {
    let dir = temp_case("parallel-bench");
    let (input, schema) = write_generated_fixture(&dir, 16);

    let bench = run(&[
        "parallel-bench",
        input.to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
        "--workers",
        "2",
        "--block-rows",
        "2",
    ]);
    assert_success(&bench);
    let stdout = String::from_utf8_lossy(&bench.stdout);

    assert!(stdout.contains("mode: parallel"));
    assert!(stdout.contains("workers: 2"));
    assert!(stdout.contains("block_rows: 2"));
    assert!(stdout.contains("blocks: 8"));
    assert!(stdout.contains("parallel_encode_mib_s:"));
    assert!(stdout.contains("parallel_decode_mib_s:"));
    assert!(stdout.contains("encode_speedup:"));
    assert!(stdout.contains("decode_speedup:"));

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

#[test]
fn search_cli_encodes_inspects_queries_and_benchmarks_dictionary() {
    let dir = temp_case("search");
    let input = dir.join("search.txt");
    let encoded = dir.join("search.cmp");
    fs::write(
        &input,
        concat!(
            "brown 1 1\n",
            "brown 3 4,8\n",
            "fox 1 2,9\n",
            "fox 2 1\n",
            "fox 3 5\n",
            "quick 1 0\n",
        ),
    )
    .unwrap();

    let encode = run(&[
        "search-encode",
        input.to_str().unwrap(),
        encoded.to_str().unwrap(),
        "--skip-step",
        "2",
    ]);
    assert_success(&encode);

    let inspect = run(&["search-inspect", encoded.to_str().unwrap()]);
    assert_success(&inspect);
    let stdout = String::from_utf8_lossy(&inspect.stdout);
    assert!(stdout.contains("format: search"));
    assert!(stdout.contains("terms: 3"));
    assert!(stdout.contains("term fox docs=3"));

    let lookup = run(&["search-lookup", encoded.to_str().unwrap(), "--term", "fox"]);
    assert_success(&lookup);
    let stdout = String::from_utf8_lossy(&lookup.stdout);
    assert!(stdout.contains("documents: 3"));
    assert!(stdout.contains("doc id=1 freq=2 positions=2,9"));

    let seek = run(&[
        "search-seek",
        encoded.to_str().unwrap(),
        "--term",
        "fox",
        "--doc-id",
        "3",
    ]);
    assert_success(&seek);
    let stdout = String::from_utf8_lossy(&seek.stdout);
    assert!(stdout.contains("found: true"));
    assert!(stdout.contains("positions: 5"));

    let bench = run(&[
        "search-bench",
        input.to_str().unwrap(),
        "--skip-step",
        "2",
        "--top-k",
        "2",
    ]);
    assert_success(&bench);
    let stdout = String::from_utf8_lossy(&bench.stdout);
    assert!(stdout.contains("mode: search"));
    assert!(stdout.contains("top_k_ms:"));

    fs::remove_dir_all(dir).ok();
}

#[test]
fn append_stream_cli_recovers_replays_rolls_benchmarks_and_snapshots() {
    let dir = temp_case("append");
    let input_a = dir.join("a.jsonl");
    let input_b = dir.join("b.jsonl");
    let schema = dir.join("schema.yml");
    let append = dir.join("append.cmp");
    let replayed = dir.join("replayed.jsonl");
    let rolled = dir.join("rolled");
    let state = dir.join("state.bin");
    let snapshot = dir.join("state.snp");
    let restored = dir.join("restored.bin");

    fs::write(&input_a, "{\"ts\":100}\n{\"ts\":101}\n").unwrap();
    fs::write(&input_b, "{\"ts\":102}\n").unwrap();
    fs::write(
        &schema,
        "columns:\n  - name: ts\n    type: u64\n    codec: delta_varint_u64\n",
    )
    .unwrap();

    assert_success(&run(&[
        "stream-append",
        input_a.to_str().unwrap(),
        append.to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
        "--block-rows",
        "1",
    ]));
    assert_success(&run(&[
        "stream-append",
        input_b.to_str().unwrap(),
        append.to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
        "--block-rows",
        "1",
    ]));

    let recover = run(&["stream-recover", append.to_str().unwrap()]);
    assert_success(&recover);
    let stdout = String::from_utf8_lossy(&recover.stdout);
    assert!(stdout.contains("format: append-stream"));
    assert!(stdout.contains("blocks: 3"));
    assert!(stdout.contains("total_rows: 3"));
    assert!(stdout.contains("truncated_or_corrupt_tail: false"));

    assert_success(&run(&[
        "stream-replay",
        append.to_str().unwrap(),
        replayed.to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
    ]));
    assert_eq!(
        fs::read_to_string(&replayed).unwrap(),
        "{\"ts\":100}\n{\"ts\":101}\n{\"ts\":102}\n"
    );

    let roll = run(&[
        "stream-roll",
        replayed.to_str().unwrap(),
        rolled.to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
        "--block-rows",
        "1",
        "--max-blocks",
        "2",
    ]);
    assert_success(&roll);
    assert!(rolled.join("segment-00000.cmp").exists());
    assert!(rolled.join("segment-00001.cmp").exists());

    let bench = run(&[
        "stream-bench",
        replayed.to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
        "--block-rows",
        "1",
    ]);
    assert_success(&bench);
    let stdout = String::from_utf8_lossy(&bench.stdout);
    assert!(stdout.contains("mode: append-stream"));
    assert!(stdout.contains("append_mib_s:"));
    assert!(stdout.contains("replay_mib_s:"));

    fs::write(&state, b"aaaaaaaaabbbbbbbbb").unwrap();
    assert_success(&run(&[
        "snapshot-encode",
        state.to_str().unwrap(),
        snapshot.to_str().unwrap(),
        "--checkpoint-id",
        "42",
    ]));
    let decode = run(&[
        "snapshot-decode",
        snapshot.to_str().unwrap(),
        restored.to_str().unwrap(),
    ]);
    assert_success(&decode);
    assert_eq!(fs::read(&restored).unwrap(), fs::read(&state).unwrap());
    assert!(String::from_utf8_lossy(&decode.stdout).contains("checkpoint_id: 42"));

    fs::remove_dir_all(dir).ok();
}

#[test]
fn failed_streaming_encode_preserves_existing_destination() {
    let dir = temp_case("atomic-encode");
    let input = dir.join("invalid.jsonl");
    let schema = dir.join("schema.yml");
    let output = dir.join("output.cmp");
    fs::write(&input, "{\"missing\":1}\n").unwrap();
    fs::write(
        &schema,
        "columns:\n  - name: ts\n    type: u64\n    codec: delta_varint_u64\n",
    )
    .unwrap();
    fs::write(&output, b"existing-output").unwrap();

    let result = run(&[
        "encode",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
    ]);

    assert_failure(&result);
    assert_eq!(fs::read(&output).unwrap(), b"existing-output");
    fs::remove_dir_all(dir).ok();
}

#[test]
fn failed_streaming_decode_preserves_existing_destination() {
    let dir = temp_case("atomic-decode");
    let (_input, schema) = write_fixture(&dir);
    let encoded = dir.join("invalid.cmp");
    let output = dir.join("output.jsonl");
    fs::write(&encoded, b"CMP2").unwrap();
    fs::write(&output, b"existing-output").unwrap();

    let result = run(&[
        "decode",
        encoded.to_str().unwrap(),
        output.to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
    ]);

    assert_failure(&result);
    assert_eq!(fs::read(&output).unwrap(), b"existing-output");
    fs::remove_dir_all(dir).ok();
}
