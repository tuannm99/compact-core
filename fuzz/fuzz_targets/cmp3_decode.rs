#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let schema = compact_core::schema::Schema::from_yaml(
        "columns:\n  - name: ts\n    type: u64\n    codec: auto\n  - name: active\n    type: bool\n    codec: auto\n  - name: path\n    type: string\n    codec: auto\n",
    )
    .expect("static fuzz schema must be valid");
    let _ = compact_core::io::v3::decode_jsonl(data, &schema);
    let _ = compact_core::io::v3::inspect_jsonl(data);
});
