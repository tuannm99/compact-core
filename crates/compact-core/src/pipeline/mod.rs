//! Composable encode/decode flows built from primitives.
//!
//! A pipeline is still below the frame layer: it turns typed values into bytes
//! and back, but it does not write magic bytes, codec IDs, lengths, or checksums.

pub mod delta_varint;
