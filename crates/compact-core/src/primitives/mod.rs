//! Small compression building blocks.
//!
//! Primitives do one narrow job and do not decide file format, schema, codec
//! selection, or framing. Higher layers compose these pieces into real codecs.

pub mod bitmap;
pub mod bitpack;
pub mod crc32;
pub mod delta;
pub mod rle;
pub mod varint;
pub mod zigzag;
