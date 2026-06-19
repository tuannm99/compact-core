//! Shared transform namespace for future cross-pipeline transforms.
//!
//! Current pipelines call primitive transforms directly, for example
//! `pipeline::delta_varint` uses `primitives::delta` before varint encoding.
//! Keep this module empty until at least two pipelines need the same
//! higher-level transform wrapper; otherwise the primitive modules remain the
//! clearer source of truth.
