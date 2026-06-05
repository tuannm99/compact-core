use crate::{CompactError, Result};

/// Default row limit for one streaming block.
///
/// This keeps metadata and per-block decode work bounded while still producing
/// blocks large enough for useful compression on structured JSONL.
pub const DEFAULT_MAX_ROWS_PER_BLOCK: usize = 10_000;

/// Default uncompressed byte limit for one streaming block.
///
/// The limit counts raw JSONL input bytes before compression. Using raw bytes
/// instead of compressed bytes gives the writer a deterministic memory budget.
pub const DEFAULT_MAX_UNCOMPRESSED_BYTES_PER_BLOCK: usize = 8 * 1024 * 1024;

/// Controls when the streaming writer flushes the current row group.
///
/// Both limits are active. The writer should flush when adding the next row
/// would exceed either the row count limit or the raw byte limit. Keeping both
/// limits prevents one pathological input shape from breaking the memory
/// contract: many tiny rows hit the row limit, while few huge rows hit the byte
/// limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockOptions {
    pub max_rows_per_block: usize,
    pub max_uncompressed_bytes_per_block: usize,
}

impl BlockOptions {
    /// Validate that both limits can enforce bounded memory.
    ///
    /// Zero is rejected because it would make every row exceed the limit and
    /// would force the writer into ambiguous flush behavior before any data can
    /// be accepted.
    pub fn validate(self) -> Result<Self> {
        if self.max_rows_per_block == 0 {
            return Err(CompactError::InvalidInput(
                "max rows per block must be greater than zero",
            ));
        }

        if self.max_uncompressed_bytes_per_block == 0 {
            return Err(CompactError::InvalidInput(
                "max uncompressed bytes per block must be greater than zero",
            ));
        }

        Ok(self)
    }
}

impl Default for BlockOptions {
    fn default() -> Self {
        Self {
            max_rows_per_block: DEFAULT_MAX_ROWS_PER_BLOCK,
            max_uncompressed_bytes_per_block: DEFAULT_MAX_UNCOMPRESSED_BYTES_PER_BLOCK,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BlockOptions, DEFAULT_MAX_ROWS_PER_BLOCK, DEFAULT_MAX_UNCOMPRESSED_BYTES_PER_BLOCK,
    };
    use crate::CompactError;

    #[test]
    fn defaults_match_v0_2_plan() {
        let options = BlockOptions::default();

        assert_eq!(options.max_rows_per_block, DEFAULT_MAX_ROWS_PER_BLOCK);
        assert_eq!(
            options.max_uncompressed_bytes_per_block,
            DEFAULT_MAX_UNCOMPRESSED_BYTES_PER_BLOCK
        );
    }

    #[test]
    fn validation_accepts_positive_limits() {
        let options = BlockOptions {
            max_rows_per_block: 1,
            max_uncompressed_bytes_per_block: 1,
        };

        assert_eq!(options.validate().unwrap(), options);
    }

    #[test]
    fn validation_rejects_zero_rows() {
        let err = BlockOptions {
            max_rows_per_block: 0,
            max_uncompressed_bytes_per_block: 1,
        }
        .validate()
        .unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("max rows per block must be greater than zero")
        ));
    }

    #[test]
    fn validation_rejects_zero_bytes() {
        let err = BlockOptions {
            max_rows_per_block: 1,
            max_uncompressed_bytes_per_block: 0,
        }
        .validate()
        .unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput(
                "max uncompressed bytes per block must be greater than zero"
            )
        ));
    }
}
