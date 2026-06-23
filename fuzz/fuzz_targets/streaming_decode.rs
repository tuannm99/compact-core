#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let schema = compact_core::schema::Schema::from_yaml(
        "columns:\n  - name: ts\n    type: u64\n    codec: delta_varint_u64\n",
    )
    .expect("static fuzz schema must be valid");

    let _ = compact_core::streaming::recover_append_stream(data);
    let _ = compact_core::streaming::replay_jsonl_append_stream(data, Vec::new(), schema);
    let _ = compact_core::streaming::decode_snapshot(data);
});
