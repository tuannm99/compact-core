# 7. Project Architecture

Recommended high-level workspace:

```text

compact-core/

compact-cli/
compact-ffi/
bindings/go/
schemas/

testdata/
benches/
docs/
```

Recommended `compact-core/src` layout:

```text
src/
  error.rs
  lib.rs
  frame/
    mod.rs
    header.rs
    reader.rs
    writer.rs
  primitives/
    mod.rs
    rle.rs
    delta.rs

    varint.rs

    zigzag.rs

    bitpack.rs
    crc32.rs

  codecs/
    mod.rs
    rle_codec.rs
    huffman.rs
    lz77.rs
  pipeline/
    mod.rs
    delta_varint.rs
    rle_frame.rs
  io/
    mod.rs
    bit_reader.rs
    bit_writer.rs
    chunk_reader.rs
```

## 7.1 Primitive vs Pipeline vs Frame

Main idea:

```text
primitives = small reusable building blocks
pipeline   = how to combine building blocks
frame      = how bytes are stored and validated
```

Example:

```text
Delta      = primitive

Varint     = primitive
CRC32      = primitive
Delta+Varint+Frame = pipeline
```

Do not put schema detection or strategy selection inside primitive loops.

## 7.2 Codec vs Transform vs Encoding

### Transform

A transform changes representation to reduce entropy.

Examples:

- Delta.
- ZigZag.
- XOR.
- BWT.

It may not reduce byte size alone.

### Encoding

Encoding converts values into bytes or bits.

Examples:

- Varint.
- Bit packing.
- Fixed-width little-endian.

### Codec

A codec compresses/decompresses a payload.

Examples:

- RLE.
- Huffman.
- LZ77.
- LZ4-like codec.

RLE is slightly special because it can be viewed as both a simple codec and a transform.

---

