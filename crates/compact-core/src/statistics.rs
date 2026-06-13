//! Versioned type-specific column statistics stored in CMP3 metadata.

use crate::{CompactError, Result};

const NONE: u8 = 0;
const U64_RANGE: u8 = 1;
const BOOL_COUNTS: u8 = 2;
const STRING_CARDINALITY: u8 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnStatistics {
    None,
    U64 { min: u64, max: u64 },
    Bool { true_count: u64 },
    String { distinct_count: u64 },
}

pub fn encode_u64(min: Option<u64>, max: Option<u64>) -> Vec<u8> {
    match (min, max) {
        (Some(min), Some(max)) => {
            let mut out = vec![U64_RANGE];
            out.extend_from_slice(&min.to_le_bytes());
            out.extend_from_slice(&max.to_le_bytes());
            out
        }
        _ => vec![NONE],
    }
}

pub fn encode_bool(true_count: u64) -> Vec<u8> {
    let mut out = vec![BOOL_COUNTS];
    out.extend_from_slice(&true_count.to_le_bytes());
    out
}

pub fn encode_string(distinct_count: u64) -> Vec<u8> {
    let mut out = vec![STRING_CARDINALITY];
    out.extend_from_slice(&distinct_count.to_le_bytes());
    out
}

pub fn decode(data: &[u8]) -> Result<ColumnStatistics> {
    match data {
        [NONE] => Ok(ColumnStatistics::None),
        [U64_RANGE, rest @ ..] if rest.len() == 16 => {
            let min = u64::from_le_bytes(rest[..8].try_into().expect("eight-byte min"));
            let max = u64::from_le_bytes(rest[8..].try_into().expect("eight-byte max"));
            if min > max {
                return Err(CompactError::InvalidInput(
                    "cmp3 u64 statistics min exceeds max",
                ));
            }
            Ok(ColumnStatistics::U64 { min, max })
        }
        [BOOL_COUNTS, rest @ ..] if rest.len() == 8 => Ok(ColumnStatistics::Bool {
            true_count: u64::from_le_bytes(rest.try_into().expect("eight-byte true count")),
        }),
        [STRING_CARDINALITY, rest @ ..] if rest.len() == 8 => Ok(ColumnStatistics::String {
            distinct_count: u64::from_le_bytes(rest.try_into().expect("eight-byte distinct count")),
        }),
        _ => Err(CompactError::InvalidInput(
            "invalid cmp3 statistics metadata",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{ColumnStatistics, decode, encode_bool, encode_string, encode_u64};

    #[test]
    fn statistics_roundtrip() {
        assert_eq!(
            decode(&encode_u64(Some(3), Some(9))).unwrap(),
            ColumnStatistics::U64 { min: 3, max: 9 }
        );
        assert_eq!(
            decode(&encode_bool(7)).unwrap(),
            ColumnStatistics::Bool { true_count: 7 }
        );
        assert_eq!(
            decode(&encode_string(4)).unwrap(),
            ColumnStatistics::String { distinct_count: 4 }
        );
    }
}
