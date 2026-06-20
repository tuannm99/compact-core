//! Posting-list compression for search-index workloads.
//!
//! A posting list stores every document that contains one term, optionally with
//! term positions inside each document. The format is sectioned so later code
//! can seek into the docID stream from a skip entry without decoding unrelated
//! documents first.

use crate::{CompactError, Result, checksum32, primitives::varint};

const MAGIC: [u8; 4] = *b"PST1";
const VERSION: u8 = 1;
const HEADER_LEN: usize = 4 + 1 + 4;

/// One document entry for a term.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Posting {
    pub doc_id: u64,
    pub positions: Vec<u64>,
}

/// Serialized skip entry used to restart scanning near a target docID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkipEntry {
    pub ordinal: u64,
    pub first_doc_id: u64,
    pub previous_doc_id: u64,
    pub doc_ids_offset: u64,
    pub frequencies_offset: u64,
    pub positions_offset: u64,
}

/// Metadata decoded from the posting-list header and skip section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostingListIndex {
    pub doc_count: u64,
    pub skip_step: u64,
    pub doc_ids_len: u64,
    pub frequencies_len: u64,
    pub positions_len: u64,
    pub skip_entries: Vec<SkipEntry>,
}

/// Encode a sorted posting list into a checked binary payload.
pub fn encode_postings(postings: &[Posting], skip_step: usize) -> Result<Vec<u8>> {
    if skip_step == 0 {
        return Err(CompactError::InvalidInput(
            "posting skip step must be non-zero",
        ));
    }

    validate_postings(postings)?;

    let mut doc_ids = Vec::new();
    let mut frequencies = Vec::new();
    let mut positions = Vec::new();
    let mut skips = Vec::new();
    let mut previous_doc_id = 0u64;

    for (ordinal, posting) in postings.iter().enumerate() {
        if ordinal % skip_step == 0 {
            skips.push(SkipEntry {
                ordinal: ordinal as u64,
                first_doc_id: posting.doc_id,
                previous_doc_id,
                doc_ids_offset: doc_ids.len() as u64,
                frequencies_offset: frequencies.len() as u64,
                positions_offset: positions.len() as u64,
            });
        }

        let doc_delta = posting.doc_id - previous_doc_id;
        doc_ids.extend_from_slice(&varint::encode_u64(&[doc_delta]));
        previous_doc_id = posting.doc_id;

        frequencies.extend_from_slice(&varint::encode_u64(&[posting.positions.len() as u64]));
        positions.extend_from_slice(&encode_position_deltas(&posting.positions));
    }

    let mut skip_bytes = Vec::new();
    for entry in &skips {
        skip_bytes.extend_from_slice(&varint::encode_u64(&[
            entry.ordinal,
            entry.first_doc_id,
            entry.previous_doc_id,
            entry.doc_ids_offset,
            entry.frequencies_offset,
            entry.positions_offset,
        ]));
    }

    let mut body = Vec::new();
    body.extend_from_slice(&varint::encode_u64(&[
        postings.len() as u64,
        skip_step as u64,
        doc_ids.len() as u64,
        frequencies.len() as u64,
        positions.len() as u64,
        skip_bytes.len() as u64,
    ]));
    body.extend_from_slice(&doc_ids);
    body.extend_from_slice(&frequencies);
    body.extend_from_slice(&positions);
    body.extend_from_slice(&skip_bytes);

    let checksum = checksum32(&body);
    let mut out = Vec::with_capacity(HEADER_LEN + body.len());
    out.extend_from_slice(&MAGIC);
    out.push(VERSION);
    out.extend_from_slice(&checksum.to_le_bytes());
    out.extend_from_slice(&body);

    Ok(out)
}

/// Decode and validate a complete posting-list payload.
pub fn decode_postings(data: &[u8]) -> Result<Vec<Posting>> {
    let parsed = ParsedPostingList::parse(data)?;
    parsed.decode_all()
}

/// Read only the searchable metadata and skip table.
pub fn inspect_postings(data: &[u8]) -> Result<PostingListIndex> {
    Ok(ParsedPostingList::parse(data)?.index)
}

/// Find one docID using the nearest skip entry instead of decoding from zero.
pub fn seek_doc(data: &[u8], target_doc_id: u64) -> Result<Option<Posting>> {
    let parsed = ParsedPostingList::parse(data)?;
    let Some(skip) = parsed.skip_before_or_at(target_doc_id) else {
        return Ok(None);
    };

    let mut doc_cursor = usize::try_from(skip.doc_ids_offset)
        .map_err(|_| CompactError::InvalidInput("posting skip offset is too large"))?;
    let mut freq_cursor = usize::try_from(skip.frequencies_offset)
        .map_err(|_| CompactError::InvalidInput("posting skip offset is too large"))?;
    let mut pos_cursor = usize::try_from(skip.positions_offset)
        .map_err(|_| CompactError::InvalidInput("posting skip offset is too large"))?;
    let mut previous_doc_id = skip.previous_doc_id;
    let mut ordinal = skip.ordinal;

    while ordinal < parsed.index.doc_count {
        let delta = varint::read_u64(parsed.doc_ids, &mut doc_cursor)?;
        let doc_id = previous_doc_id
            .checked_add(delta)
            .ok_or(CompactError::InvalidInput("posting docID decode overflow"))?;
        let frequency = varint::read_u64(parsed.frequencies, &mut freq_cursor)?;
        let positions = read_positions(parsed.positions, &mut pos_cursor, frequency)?;

        if doc_id == target_doc_id {
            return Ok(Some(Posting { doc_id, positions }));
        }

        if doc_id > target_doc_id {
            return Ok(None);
        }

        previous_doc_id = doc_id;
        ordinal += 1;
    }

    Ok(None)
}

struct ParsedPostingList<'a> {
    index: PostingListIndex,
    doc_ids: &'a [u8],
    frequencies: &'a [u8],
    positions: &'a [u8],
}

impl<'a> ParsedPostingList<'a> {
    fn parse(data: &'a [u8]) -> Result<Self> {
        if data.len() < HEADER_LEN {
            return Err(CompactError::InvalidInput("posting payload is truncated"));
        }

        if data[..4] != MAGIC {
            return Err(CompactError::InvalidInput(
                "posting payload has invalid magic",
            ));
        }

        if data[4] != VERSION {
            return Err(CompactError::Unsupported("posting payload version"));
        }

        let expected_checksum = u32::from_le_bytes(
            data[5..9]
                .try_into()
                .expect("posting checksum slice has fixed length"),
        );
        let body = &data[HEADER_LEN..];
        if checksum32(body) != expected_checksum {
            return Err(CompactError::InvalidInput("posting checksum mismatch"));
        }

        let mut cursor = 0usize;
        let doc_count = varint::read_u64(body, &mut cursor)?;
        let skip_step = varint::read_u64(body, &mut cursor)?;
        let doc_ids_len = varint::read_u64(body, &mut cursor)?;
        let frequencies_len = varint::read_u64(body, &mut cursor)?;
        let positions_len = varint::read_u64(body, &mut cursor)?;
        let skip_len = varint::read_u64(body, &mut cursor)?;

        if skip_step == 0 {
            return Err(CompactError::InvalidInput(
                "posting skip step must be non-zero",
            ));
        }

        let doc_ids = take_section(
            body,
            &mut cursor,
            doc_ids_len,
            "posting docIDs are truncated",
        )?;
        let frequencies = take_section(
            body,
            &mut cursor,
            frequencies_len,
            "posting frequencies are truncated",
        )?;
        let positions = take_section(
            body,
            &mut cursor,
            positions_len,
            "posting positions are truncated",
        )?;
        let skip_bytes = take_section(
            body,
            &mut cursor,
            skip_len,
            "posting skip table is truncated",
        )?;

        if cursor != body.len() {
            return Err(CompactError::InvalidInput(
                "posting payload has trailing bytes",
            ));
        }

        let skip_entries = decode_skips(skip_bytes)?;
        validate_index(
            doc_count,
            skip_step,
            doc_ids_len,
            frequencies_len,
            positions_len,
            &skip_entries,
        )?;

        Ok(Self {
            index: PostingListIndex {
                doc_count,
                skip_step,
                doc_ids_len,
                frequencies_len,
                positions_len,
                skip_entries,
            },
            doc_ids,
            frequencies,
            positions,
        })
    }

    fn decode_all(&self) -> Result<Vec<Posting>> {
        let mut postings = Vec::with_capacity(
            usize::try_from(self.index.doc_count)
                .map_err(|_| CompactError::InvalidInput("posting count is too large"))?,
        );
        let mut doc_cursor = 0usize;
        let mut freq_cursor = 0usize;
        let mut pos_cursor = 0usize;
        let mut previous_doc_id = 0u64;

        for _ in 0..self.index.doc_count {
            let delta = varint::read_u64(self.doc_ids, &mut doc_cursor)?;
            let doc_id = previous_doc_id
                .checked_add(delta)
                .ok_or(CompactError::InvalidInput("posting docID decode overflow"))?;
            let frequency = varint::read_u64(self.frequencies, &mut freq_cursor)?;
            let positions = read_positions(self.positions, &mut pos_cursor, frequency)?;

            postings.push(Posting { doc_id, positions });
            previous_doc_id = doc_id;
        }

        if doc_cursor != self.doc_ids.len()
            || freq_cursor != self.frequencies.len()
            || pos_cursor != self.positions.len()
        {
            return Err(CompactError::InvalidInput(
                "posting section has trailing bytes",
            ));
        }

        validate_postings(&postings)?;

        Ok(postings)
    }

    fn skip_before_or_at(&self, target_doc_id: u64) -> Option<&SkipEntry> {
        self.index
            .skip_entries
            .partition_point(|entry| entry.first_doc_id <= target_doc_id)
            .checked_sub(1)
            .and_then(|idx| self.index.skip_entries.get(idx))
    }
}

fn validate_postings(postings: &[Posting]) -> Result<()> {
    let mut previous_doc_id = None;

    for posting in postings {
        if let Some(previous) = previous_doc_id
            && posting.doc_id <= previous
        {
            return Err(CompactError::InvalidInput(
                "posting docIDs must be strictly increasing",
            ));
        }

        validate_positions(&posting.positions)?;
        previous_doc_id = Some(posting.doc_id);
    }

    Ok(())
}

fn validate_positions(positions: &[u64]) -> Result<()> {
    for window in positions.windows(2) {
        if window[1] <= window[0] {
            return Err(CompactError::InvalidInput(
                "posting positions must be strictly increasing",
            ));
        }
    }

    Ok(())
}

fn encode_position_deltas(positions: &[u64]) -> Vec<u8> {
    let mut deltas = Vec::with_capacity(positions.len());
    let mut previous = 0u64;

    for &position in positions {
        deltas.push(position - previous);
        previous = position;
    }

    varint::encode_u64(&deltas)
}

fn read_positions(data: &[u8], cursor: &mut usize, count: u64) -> Result<Vec<u64>> {
    let capacity = usize::try_from(count)
        .map_err(|_| CompactError::InvalidInput("posting frequency is too large"))?;
    let mut positions = Vec::with_capacity(capacity);
    let mut previous = 0u64;

    for _ in 0..count {
        let delta = varint::read_u64(data, cursor)?;
        let position = previous
            .checked_add(delta)
            .ok_or(CompactError::InvalidInput(
                "posting position decode overflow",
            ))?;
        positions.push(position);
        previous = position;
    }

    validate_positions(&positions)?;

    Ok(positions)
}

fn decode_skips(data: &[u8]) -> Result<Vec<SkipEntry>> {
    let mut cursor = 0usize;
    let mut entries = Vec::new();

    while cursor < data.len() {
        entries.push(SkipEntry {
            ordinal: varint::read_u64(data, &mut cursor)?,
            first_doc_id: varint::read_u64(data, &mut cursor)?,
            previous_doc_id: varint::read_u64(data, &mut cursor)?,
            doc_ids_offset: varint::read_u64(data, &mut cursor)?,
            frequencies_offset: varint::read_u64(data, &mut cursor)?,
            positions_offset: varint::read_u64(data, &mut cursor)?,
        });
    }

    Ok(entries)
}

fn validate_index(
    doc_count: u64,
    skip_step: u64,
    doc_ids_len: u64,
    frequencies_len: u64,
    positions_len: u64,
    skip_entries: &[SkipEntry],
) -> Result<()> {
    let expected_skips = if doc_count == 0 {
        0
    } else {
        doc_count.div_ceil(skip_step)
    };

    if skip_entries.len() as u64 != expected_skips {
        return Err(CompactError::InvalidInput("posting skip count mismatch"));
    }

    let mut previous_first_doc_id = None;
    for (idx, entry) in skip_entries.iter().enumerate() {
        let expected_ordinal = idx as u64 * skip_step;
        if entry.ordinal != expected_ordinal {
            return Err(CompactError::InvalidInput("posting skip ordinal mismatch"));
        }

        if entry.ordinal >= doc_count {
            return Err(CompactError::InvalidInput(
                "posting skip ordinal out of range",
            ));
        }

        if let Some(previous) = previous_first_doc_id
            && entry.first_doc_id <= previous
        {
            return Err(CompactError::InvalidInput(
                "posting skip docIDs must be increasing",
            ));
        }

        if entry.doc_ids_offset > doc_ids_len
            || entry.frequencies_offset > frequencies_len
            || entry.positions_offset > positions_len
        {
            return Err(CompactError::InvalidInput(
                "posting skip offset out of range",
            ));
        }

        previous_first_doc_id = Some(entry.first_doc_id);
    }

    Ok(())
}

fn take_section<'a>(
    data: &'a [u8],
    cursor: &mut usize,
    len: u64,
    message: &'static str,
) -> Result<&'a [u8]> {
    let len = usize::try_from(len)
        .map_err(|_| CompactError::InvalidInput("posting section is too large"))?;
    let end = cursor.checked_add(len).ok_or(CompactError::InvalidInput(
        "posting section length overflow",
    ))?;
    let section = data
        .get(*cursor..end)
        .ok_or(CompactError::InvalidInput(message))?;
    *cursor = end;

    Ok(section)
}

#[cfg(test)]
mod tests {
    use super::{Posting, decode_postings, encode_postings, inspect_postings, seek_doc};
    use crate::CompactError;

    fn sample_postings() -> Vec<Posting> {
        vec![
            Posting {
                doc_id: 1,
                positions: vec![0, 8, 12],
            },
            Posting {
                doc_id: 3,
                positions: vec![2],
            },
            Posting {
                doc_id: 10,
                positions: vec![1, 9],
            },
            Posting {
                doc_id: 15,
                positions: Vec::new(),
            },
            Posting {
                doc_id: 30,
                positions: vec![4, 64, 1024],
            },
        ]
    }

    #[test]
    fn posting_roundtrip_preserves_doc_ids_and_positions() {
        let postings = sample_postings();
        let encoded = encode_postings(&postings, 2).unwrap();
        let decoded = decode_postings(&encoded).unwrap();

        assert_eq!(decoded, postings);
    }

    #[test]
    fn empty_posting_list_roundtrips_without_skips() {
        let encoded = encode_postings(&[], 4).unwrap();
        let decoded = decode_postings(&encoded).unwrap();
        let index = inspect_postings(&encoded).unwrap();

        assert!(decoded.is_empty());
        assert_eq!(index.doc_count, 0);
        assert!(index.skip_entries.is_empty());
    }

    #[test]
    fn inspect_returns_skip_metadata_without_decoding_postings() {
        let encoded = encode_postings(&sample_postings(), 2).unwrap();
        let index = inspect_postings(&encoded).unwrap();

        assert_eq!(index.doc_count, 5);
        assert_eq!(index.skip_step, 2);
        assert_eq!(index.skip_entries.len(), 3);
        assert_eq!(index.skip_entries[0].ordinal, 0);
        assert_eq!(index.skip_entries[0].first_doc_id, 1);
        assert_eq!(index.skip_entries[1].ordinal, 2);
        assert_eq!(index.skip_entries[1].first_doc_id, 10);
        assert_eq!(index.skip_entries[2].ordinal, 4);
        assert_eq!(index.skip_entries[2].first_doc_id, 30);
    }

    #[test]
    fn seek_doc_uses_skip_table_to_find_or_miss_doc_id() {
        let postings = sample_postings();
        let encoded = encode_postings(&postings, 2).unwrap();

        assert_eq!(seek_doc(&encoded, 10).unwrap(), Some(postings[2].clone()));
        assert_eq!(seek_doc(&encoded, 30).unwrap(), Some(postings[4].clone()));
        assert_eq!(seek_doc(&encoded, 2).unwrap(), None);
        assert_eq!(seek_doc(&encoded, 31).unwrap(), None);
    }

    #[test]
    fn encoder_rejects_zero_skip_step() {
        let err = encode_postings(&sample_postings(), 0).unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("posting skip step must be non-zero")
        ));
    }

    #[test]
    fn encoder_rejects_unsorted_doc_ids() {
        let err = encode_postings(
            &[
                Posting {
                    doc_id: 3,
                    positions: Vec::new(),
                },
                Posting {
                    doc_id: 3,
                    positions: Vec::new(),
                },
            ],
            2,
        )
        .unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("posting docIDs must be strictly increasing")
        ));
    }

    #[test]
    fn encoder_rejects_unsorted_positions() {
        let err = encode_postings(
            &[Posting {
                doc_id: 1,
                positions: vec![1, 1],
            }],
            2,
        )
        .unwrap_err();

        assert!(matches!(
            err,
            CompactError::InvalidInput("posting positions must be strictly increasing")
        ));
    }

    #[test]
    fn decoder_rejects_bad_magic_and_checksum() {
        let mut encoded = encode_postings(&sample_postings(), 2).unwrap();
        encoded[0] = b'X';
        let err = decode_postings(&encoded).unwrap_err();
        assert!(matches!(
            err,
            CompactError::InvalidInput("posting payload has invalid magic")
        ));

        let mut encoded = encode_postings(&sample_postings(), 2).unwrap();
        let last = encoded.len() - 1;
        encoded[last] ^= 0x01;
        let err = decode_postings(&encoded).unwrap_err();
        assert!(matches!(
            err,
            CompactError::InvalidInput("posting checksum mismatch")
        ));
    }
}
