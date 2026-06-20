//! Search-index compression helpers.
//!
//! v0.5 starts with posting-list payloads because inverted indexes depend on
//! compact, seekable `(doc_id, positions)` lists before they need a full file
//! format or query engine integration.

pub mod dictionary;
pub mod postings;
pub mod query;
