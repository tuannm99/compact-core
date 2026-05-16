use crate::CompactError;

/*REVIEWER [BLOCKER][CORRECTNESS]: this is fixed-width little-endian serialization, not varint encoding.
WHY: a varint codec must emit 1..10 bytes per value depending on magnitude, but `to_le_bytes()` always emits 8 bytes and produces a different wire format.
FIX: implement standard base-128 varint emission by writing 7 data bits per byte and setting the continuation bit until the remaining value fits in one byte.
*/
pub fn encode_u64(values: &[u64]) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() * 8);

    for &value in values {
        out.extend_from_slice(&value.to_le_bytes());
    }

    out
}

/*REVIEWER [BLOCKER][API_DESIGN]: this decoder does not decode bytes into integers.
WHY: accepting `&[u64]` and returning `Vec<u8>` makes this function another encoder, so there is no decode path for serialized varints.
FIX: change the signature to accept encoded bytes, e.g. `decode_u64(data: &[u8]) -> Result<Vec<u64>, CompactError>`, and validate truncated or overflowing sequences.
*/
pub fn decode_u64(values: &[u8]) -> Result<Vec<u64>, CompactError> {
    let mut out = Vec::with_capacity(values.len() * 8);

    // for &value in values {
    //     out.extend_from_slice(&value.to_le_bytes());
    // }

    Ok(out)
}

#[cfg(test)]
mod tests {
    /*REVIEWER [BLOCKER][TESTING]: this new binary codec has no tests.
    WHY: without exact-encoding, roundtrip, and malformed-input coverage, it is easy to ship an incompatible wire format or a decoder that panics or accepts corrupted input.
    FIX: add unit tests for canonical varint encodings, roundtrips across edge values, and rejection of truncated or overflowing inputs.
    */
}
