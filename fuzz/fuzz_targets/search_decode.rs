#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = compact_core::search::postings::inspect_postings(data);
    let _ = compact_core::search::postings::decode_postings(data);
    let _ = compact_core::search::postings::seek_doc(data, 42);

    let _ = compact_core::search::dictionary::inspect_dictionary(data);
    let _ = compact_core::search::dictionary::lookup_term(data, "term");
    let _ = compact_core::search::dictionary::seek_term_doc(data, "term", 42);
    let _ = compact_core::search::dictionary::inspect_term_postings(data, "term");

    let _ = compact_core::search::query::term_doc_ids(data, "term");
    let _ = compact_core::search::query::and_doc_ids(data, &["term", "other"]);
    let _ = compact_core::search::query::has_adjacent_phrase(data, "term", "other", 42);
    let _ = compact_core::search::query::top_k_by_term_frequency(data, &["term", "other"], 3);
    let _ = compact_core::search::query::term_contains_doc(data, "term", 42);
});
