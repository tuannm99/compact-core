//! Copy-on-write repair planning for recoverable storage files.
//!
//! A plan is bound to the source length and checksum. Execution rejects a plan
//! if the caller supplies different bytes, preventing a stale plan from
//! truncating a file that changed between inspection and repair.

use std::io::Cursor;

use super::{StorageFormat, detect};
use crate::streaming::recover_append_stream;
use crate::{CompactError, Result, checksum32};

/// The exact mutation a repair operation will apply to a new output buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairAction {
    /// The file already has a valid footer and requires no byte changes.
    None,
    /// Preserve all valid blocks and add a missing footer index.
    RebuildFooter,
    /// Discard a corrupt or partial tail, then add a footer index.
    TruncateTailAndRebuildFooter,
}

/// Immutable repair decision produced before output bytes are written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairPlan {
    pub format: StorageFormat,
    pub action: RepairAction,
    pub source_len: u64,
    pub source_checksum: u32,
    pub recoverable_len: u64,
    pub discarded_bytes: u64,
    pub recovered_units: u64,
    pub recovered_rows: u64,
}

/// Inspect a file and return the repair operation without changing any bytes.
///
/// CMP2 and CMP4 are repairable because both provide independently checksummed
/// storage units from which a trustworthy contiguous prefix can be rebuilt.
pub fn plan(data: &[u8]) -> Result<RepairPlan> {
    let format = detect(data)?;
    let source_len = u64::try_from(data.len())
        .map_err(|_| CompactError::InvalidInput("repair source is too large"))?;
    let source_checksum = checksum32(data);

    match format {
        StorageFormat::V2 => plan_cmp2(data, source_len, source_checksum),
        StorageFormat::V4 => plan_cmp4(data, source_len, source_checksum),
        StorageFormat::V1 | StorageFormat::V3 => Err(CompactError::Unsupported(
            "repair supports cmp2 and cmp4 files",
        )),
    }
}

/// Execute a previously reviewed plan into a new byte buffer.
///
/// The input slice is never modified. Callers should write the returned bytes
/// to a different path and atomically replace the original only after their own
/// durability policy is satisfied.
pub fn execute(data: &[u8], plan: &RepairPlan) -> Result<Vec<u8>> {
    let source_len = u64::try_from(data.len())
        .map_err(|_| CompactError::InvalidInput("repair source is too large"))?;
    if source_len != plan.source_len || checksum32(data) != plan.source_checksum {
        return Err(CompactError::InvalidInput(
            "repair source does not match plan",
        ));
    }
    if detect(data)? != plan.format {
        return Err(CompactError::InvalidInput(
            "repair source format does not match plan",
        ));
    }

    if plan.action == RepairAction::None {
        return Ok(data.to_vec());
    }

    match plan.format {
        StorageFormat::V2 => {
            let recovery = recover_append_stream(data)?;
            if recovery.valid_len != plan.recoverable_len
                || recovery.total_rows != plan.recovered_rows
                || recovery.blocks.len() as u64 != plan.recovered_units
            {
                return Err(CompactError::InvalidInput(
                    "repair recovery no longer matches plan",
                ));
            }

            let prefix_len = usize::try_from(plan.recoverable_len)
                .map_err(|_| CompactError::InvalidInput("repair prefix is too large"))?;
            let mut output = data[..prefix_len].to_vec();
            crate::streaming::writer::write_index_footer(&mut output, &recovery.blocks)?;

            let inspect = crate::streaming::inspect_stream(Cursor::new(&output))?;
            if inspect.footer_index.is_none() {
                return Err(CompactError::InvalidInput(
                    "repaired cmp2 footer is missing",
                ));
            }
            Ok(output)
        }
        StorageFormat::V4 => {
            let recovery = crate::io::v4::recover_file_prefix(data)?;
            if recovery.valid_body_len != plan.recoverable_len
                || recovery.footer.total_row_count != plan.recovered_rows
                || recovery.footer.row_groups.len() as u64 != plan.recovered_units
            {
                return Err(CompactError::InvalidInput(
                    "repair recovery no longer matches plan",
                ));
            }

            let prefix_len = usize::try_from(plan.recoverable_len)
                .map_err(|_| CompactError::InvalidInput("repair prefix is too large"))?;
            let mut output = data[..prefix_len].to_vec();
            crate::format::v4::append_footer(&mut output, &recovery.footer)?;
            crate::io::v4::validate_file(&output)?;
            Ok(output)
        }
        StorageFormat::V1 | StorageFormat::V3 => Err(CompactError::Unsupported(
            "repair supports cmp2 and cmp4 files",
        )),
    }
}

fn plan_cmp2(data: &[u8], source_len: u64, source_checksum: u32) -> Result<RepairPlan> {
    if let Ok(inspect) = crate::streaming::inspect_stream(Cursor::new(data))
        && inspect.footer_index.is_some()
    {
        return no_op_plan(
            StorageFormat::V2,
            source_len,
            source_checksum,
            inspect.blocks.len(),
            inspect.total_rows,
        );
    }

    let recovery = recover_append_stream(data)?;
    if recovery.valid_len < 10 {
        return Err(CompactError::InvalidInput("cmp2 header is not recoverable"));
    }
    repair_plan(
        StorageFormat::V2,
        source_len,
        source_checksum,
        recovery.valid_len,
        recovery.blocks.len(),
        recovery.total_rows,
    )
}

fn plan_cmp4(data: &[u8], source_len: u64, source_checksum: u32) -> Result<RepairPlan> {
    if let Ok(footer) = crate::io::v4::validate_file(data) {
        return no_op_plan(
            StorageFormat::V4,
            source_len,
            source_checksum,
            footer.row_groups.len(),
            footer.total_row_count,
        );
    }

    let recovery = crate::io::v4::recover_file_prefix(data)?;
    repair_plan(
        StorageFormat::V4,
        source_len,
        source_checksum,
        recovery.valid_body_len,
        recovery.footer.row_groups.len(),
        recovery.footer.total_row_count,
    )
}

fn no_op_plan(
    format: StorageFormat,
    source_len: u64,
    source_checksum: u32,
    recovered_units: usize,
    recovered_rows: u64,
) -> Result<RepairPlan> {
    Ok(RepairPlan {
        format,
        action: RepairAction::None,
        source_len,
        source_checksum,
        recoverable_len: source_len,
        discarded_bytes: 0,
        recovered_units: u64::try_from(recovered_units)
            .map_err(|_| CompactError::InvalidInput("storage unit count is too large"))?,
        recovered_rows,
    })
}

fn repair_plan(
    format: StorageFormat,
    source_len: u64,
    source_checksum: u32,
    recoverable_len: u64,
    recovered_units: usize,
    recovered_rows: u64,
) -> Result<RepairPlan> {
    let discarded_bytes = source_len
        .checked_sub(recoverable_len)
        .ok_or(CompactError::InvalidInput("repair length underflow"))?;
    let action = if discarded_bytes == 0 {
        RepairAction::RebuildFooter
    } else {
        RepairAction::TruncateTailAndRebuildFooter
    };
    Ok(RepairPlan {
        format,
        action,
        source_len,
        source_checksum,
        recoverable_len,
        discarded_bytes,
        recovered_units: u64::try_from(recovered_units)
            .map_err(|_| CompactError::InvalidInput("storage unit count is too large"))?,
        recovered_rows,
    })
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{RepairAction, execute, plan};
    use crate::CompactError;
    use crate::schema::Schema;
    use crate::streaming::BlockOptions;

    fn schema() -> Schema {
        Schema::from_yaml("columns:\n  - name: id\n    type: u64\n    codec: delta_varint_u64\n")
            .unwrap()
    }

    fn append_file() -> Vec<u8> {
        crate::streaming::append_jsonl_stream(
            &[],
            Cursor::new("{\"id\":1}\n{\"id\":2}\n{\"id\":3}\n"),
            schema(),
            BlockOptions {
                max_rows_per_block: 1,
                ..BlockOptions::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn seals_a_valid_append_stream_without_discarding_blocks() {
        let source = append_file();
        let repair_plan = plan(&source).unwrap();
        let repaired = execute(&source, &repair_plan).unwrap();
        let inspect = crate::streaming::inspect_stream(Cursor::new(repaired)).unwrap();

        assert_eq!(repair_plan.action, RepairAction::RebuildFooter);
        assert_eq!(repair_plan.discarded_bytes, 0);
        assert_eq!(inspect.blocks.len(), 3);
        assert!(inspect.footer_index.is_some());
    }

    #[test]
    fn discards_only_the_corrupt_tail_and_rebuilds_footer() {
        let mut source = append_file();
        *source.last_mut().unwrap() ^= 0xff;
        let repair_plan = plan(&source).unwrap();
        let repaired = execute(&source, &repair_plan).unwrap();
        let inspect = crate::streaming::inspect_stream(Cursor::new(repaired)).unwrap();

        assert_eq!(
            repair_plan.action,
            RepairAction::TruncateTailAndRebuildFooter
        );
        assert_eq!(repair_plan.recovered_units, 2);
        assert_eq!(repair_plan.recovered_rows, 2);
        assert!(repair_plan.discarded_bytes > 0);
        assert_eq!(inspect.blocks.len(), 2);
    }

    #[test]
    fn stale_plan_cannot_repair_changed_source() {
        let source = append_file();
        let repair_plan = plan(&source).unwrap();
        let mut changed = source;
        changed.push(0);

        let error = execute(&changed, &repair_plan).unwrap_err();
        assert!(matches!(
            error,
            CompactError::InvalidInput("repair source does not match plan")
        ));
    }

    #[test]
    fn rebuilds_cmp4_footer_without_losing_valid_row_groups() {
        let input = "{\"id\":1}\n{\"id\":2}\n{\"id\":3}\n";
        let mut source = crate::io::v4::encode_jsonl(
            input,
            &schema(),
            crate::io::v4::EncodeOptions { row_group_rows: 1 },
        )
        .unwrap();
        *source.last_mut().unwrap() ^= 0xff;

        let repair_plan = plan(&source).unwrap();
        let repaired = execute(&source, &repair_plan).unwrap();
        let footer = crate::io::v4::validate_file(&repaired).unwrap();

        assert_eq!(
            repair_plan.action,
            RepairAction::TruncateTailAndRebuildFooter
        );
        assert_eq!(repair_plan.recovered_units, 3);
        assert_eq!(repair_plan.recovered_rows, 3);
        assert_eq!(footer.row_groups.len(), 3);
    }

    #[test]
    fn cmp4_repair_discards_corrupt_row_group_and_everything_after_it() {
        let input = "{\"id\":1}\n{\"id\":2}\n{\"id\":3}\n";
        let mut source = crate::io::v4::encode_jsonl(
            input,
            &schema(),
            crate::io::v4::EncodeOptions { row_group_rows: 1 },
        )
        .unwrap();
        let footer = crate::io::v4::inspect_footer(&source).unwrap();
        let corrupt_at = footer.row_groups[1].columns[0].payload_offset as usize;
        source[corrupt_at] ^= 0xff;

        let repair_plan = plan(&source).unwrap();
        let repaired = execute(&source, &repair_plan).unwrap();
        let repaired_footer = crate::io::v4::validate_file(&repaired).unwrap();

        assert_eq!(repair_plan.recovered_units, 1);
        assert_eq!(repair_plan.recovered_rows, 1);
        assert_eq!(repaired_footer.row_groups.len(), 1);
    }
}
