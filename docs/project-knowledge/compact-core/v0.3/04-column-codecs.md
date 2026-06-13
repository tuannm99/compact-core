# Column Codecs

## Numeric Bit Packing

For each block:

1. Optionally transform values, for example delta encode monotonic `u64`.
2. Find the maximum encoded value.
3. Compute the minimum bit width required.
4. Store the bit width in codec metadata.
5. Pack values using the existing bitpack primitive.

Required widths:

- `0`: all encoded values are zero.
- `1`: boolean-like numeric data.
- `64`: full-width values.

Compare at least:

- Delta-varint.
- Delta-bitpack.
- Stored fixed-width values.

## Implemented Numeric Metadata

The numeric codecs are implemented in
`crates/compact-core/src/codecs/v3/numeric.rs`.

- `bitpack`: `bit_width: u8`, then `non_null_count: u64`.
- `delta_bitpack`: `bit_width: u8`, `base: u64`, then
  `non_null_count: u64`.
- `delta_varint_u64`: `non_null_count: u64`.
- `stored`: `non_null_count: u64`, followed by fixed-width little-endian
  values.

Nullable numeric payloads begin with a validity bitmap. Codec payloads contain
only non-null values. Delta codecs operate over the non-null sequence and
require monotonic values.

## Boolean Bitmap

Boolean values use one value bit per non-null row.

Nullable booleans use two logical bitmaps:

- Validity bitmap.
- Value bitmap for non-null values.

Do not use one bitmap with three states. Separate bitmaps keep the format
simple and reuse nullability logic across types.

## Prefix String Compression

Prefix compression follows row order:

```text
previous string
common prefix length
current suffix length
current suffix bytes
```

The first string uses prefix length zero.

Prefix lengths operate on UTF-8 bytes. Decode must reconstruct bytes and then
validate UTF-8 before returning a string.

Reset prefix state at every block.

Good inputs:

- Sorted paths.
- Repeated service names with shared prefixes.
- URLs and hierarchical identifiers.

Bad inputs:

- Random UUIDs.
- Unrelated messages.
- Encrypted or compressed text.

Bad inputs must fall back to stored or another codec.

## Dictionary Encoding

Dictionary encoding remains block-local.

Track:

- Distinct entry count.
- Dictionary bytes.
- Encoded ID bytes.
- Total column chunk size.

High cardinality can make dictionary encoding larger than stored strings. Set a
bounded threshold and stop building the candidate when it cannot win.

## Stored Codec

Stored is the correctness and size fallback.

It still needs unambiguous framing:

- Numeric: fixed-width little-endian values.
- Boolean: bitmap.
- String: length-prefixed UTF-8 bytes.

Stored payloads remain checksummed by the surrounding block frame.
