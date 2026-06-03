# 5. Systems Engineering

This is what separates a toy compressor from a production-grade engine.

## 5.1 File Format Design

A compression format must define how bytes are organized.

Need to design:

- Magic bytes.
- Versioning.
- Codec ID.
- Flags.
- Metadata.
- Blocks.
- Checksums.
- Footer or index, if needed.

Example shape:

```text
CMP1
[header]

[blocks]
[checksum]

```

More concrete frame:

```text
[magic][version][codec][flags][raw_len][payload_len][checksum][payload]
```

Each field must define:

```text
size
meaning
endianness
validation rule
```

Example:

```text

magic       = 4 bytes: "CMP1"
version     = 1 byte
codec       = 1 byte
flags       = 1 byte
raw_len     = u64 little-endian
payload_len = u64 little-endian
checksum    = u32 little-endian
payload     = payload_len bytes
```

Without a defined format, decode cannot know what the bytes mean.

## 5.2 Versioning

Versioning allows format evolution.

Example:

```text
version = 1
```

Decoder behavior:

- If version is supported, decode.
- If version is unknown, return an unsupported version error.

Do not silently decode unknown versions.

## 5.3 Codec IDs

A frame should identify the codec or pipeline used.

Example:

```text
0x01 = RLE
0x02 = DeltaVarintU64

0x03 = Huffman
0x04 = LZ77Huffman
```

Decoder uses codec ID to dispatch:

```rust
match codec {
    Codec::Rle => decode_rle(payload),
    Codec::DeltaVarintU64 => decode_delta_varint_u64(payload),

    _ => Err(CompactError::UnsupportedCodec),
}
```

## 5.4 Checksums and Integrity

Must understand:

- CRC32.
- xxHash.
- cityhash.

Why it matters:

- Corruption must be detectable.

- Invalid input must fail safely.
- A storage format without integrity strategy is incomplete.

Minimum requirement:

```text
CRC32 over compressed payload
```

Better later:

```text
CRC32 over header + payload
or separate header checksum and block checksum
```

## 5.5 Decode Safety

Decoder must never trust input.

Validate:

- Magic bytes.
- Version.
- Codec ID.
- Payload length.

- Raw length.
- Checksum.

- Malformed varint.
- Truncated frame.
- Decompressed size limit.

Important rule:

```text
Decode must return errors, not panic.

```

## 5.6 Streaming Compression

Streaming compression is often more important than one-shot file compression.

Relevant systems:

- Kafka.

- ClickHouse-style ingestion.
- WAL.
- Network packets.
- Log processing.

Streaming design:

```text
read chunk
compress chunk
write frame
repeat
```

Decoder:

```text
read frame
validate frame
decode payload
emit output
repeat
```

## 5.7 Parallel Compression

Important later-stage topics:

- Chunk parallelism.

- Dictionary sharing.
- Thread pools.
- Lock-free queues.
- Ordered output reconstruction.

Simplest model:

```text
split input into independent chunks
compress chunks in parallel
write chunks in original order

```

Tradeoff:

- Faster throughput.
- Potentially worse compression ratio if chunks cannot share history.

---

