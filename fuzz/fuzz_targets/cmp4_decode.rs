#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let schema = compact_core::schema::Schema::from_yaml(
        "columns:\n  - name: id\n    type: u64\n    codec: auto\n  - name: active\n    type: bool\n    codec: auto\n  - name: service\n    type: string\n    codec: auto\n    nullable: true\n",
    )
    .expect("static fuzz schema must be valid");
    let _ = compact_core::io::v4::inspect_footer(data);
    let _ = compact_core::io::v4::decode_jsonl(data, &schema);
    let _ = compact_core::io::v4::decode_jsonl_projected(data, &schema, &["id"]);
    let predicate = compact_core::io::v4::Predicate::U64 {
        column: "id".to_owned(),
        op: compact_core::io::v4::U64PredicateOp::Ge(10),
    };
    let _ = compact_core::io::v4::scan_jsonl(data, &schema, &["id"], Some(&predicate));
});
