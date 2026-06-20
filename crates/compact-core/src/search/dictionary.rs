//! Term dictionary blocks for compressed inverted indexes.
//!
//! A dictionary block stores many sorted terms and points each term at one
//! independently checked posting-list payload. Readers can binary-search term
//! metadata, then decode or seek inside only that term's posting list.

use crate::{
    CompactError, Result, checksum32,
    primitives::varint,
    search::postings::{Posting, PostingListIndex, decode_postings, encode_postings, seek_doc},
};

const MAGIC: [u8; 4] = *b"TRM1";
const VERSION: u8 = 1;
const HEADER_LEN: usize = 4 + 1 + 4;

/// In-memory term and its posting list before encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermPostingList {
    pub term: String,
    pub postings: Vec<Posting>,
}

/// Metadata for one term inside a dictionary block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermIndexEntry {
    pub term: String,
    pub postings_offset: u64,
    pub postings_len: u64,
    pub doc_count: u64,
}

/// Metadata decoded without expanding posting-list payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermDictionaryIndex {
    pub term_count: u64,
    pub postings_bytes: u64,
    pub entries: Vec<TermIndexEntry>,
}

/// Encode sorted term posting lists into one reusable dictionary block.
pub fn encode_dictionary(entries: &[TermPostingList], skip_step: usize) -> Result<Vec<u8>> {
    validate_terms(entries)?;

    let mut dictionary = Vec::new();
    let mut postings_blob = Vec::new();

    for entry in entries {
        let encoded_postings = encode_postings(&entry.postings, skip_step)?;
        let postings_offset = postings_blob.len() as u64;
        let postings_len = encoded_postings.len() as u64;

        dictionary.extend_from_slice(&varint::encode_u64(&[entry.term.len() as u64]));
        dictionary.extend_from_slice(entry.term.as_bytes());
        dictionary.extend_from_slice(&varint::encode_u64(&[
            postings_offset,
            postings_len,
            entry.postings.len() as u64,
        ]));
        postings_blob.extend_from_slice(&encoded_postings);
    }

    let mut body = Vec::new();
    body.extend_from_slice(&varint::encode_u64(&[
        entries.len() as u64,
        dictionary.len() as u64,
        postings_blob.len() as u64,
    ]));
    body.extend_from_slice(&dictionary);
    body.extend_from_slice(&postings_blob);

    let checksum = checksum32(&body);
    let mut out = Vec::with_capacity(HEADER_LEN + body.len());
    out.extend_from_slice(&MAGIC);
    out.push(VERSION);
    out.extend_from_slice(&checksum.to_le_bytes());
    out.extend_from_slice(&body);

    Ok(out)
}

/// Inspect dictionary metadata without decoding any posting-list payload.
pub fn inspect_dictionary(data: &[u8]) -> Result<TermDictionaryIndex> {
    Ok(ParsedDictionary::parse(data)?.index)
}

/// Decode postings for one term. Missing terms return `Ok(None)`.
pub fn lookup_term(data: &[u8], term: &str) -> Result<Option<Vec<Posting>>> {
    let parsed = ParsedDictionary::parse(data)?;
    let Some(entry) = parsed.find_term(term) else {
        return Ok(None);
    };

    decode_postings(parsed.posting_payload(entry)).map(Some)
}

/// Find one document inside one term's posting list.
pub fn seek_term_doc(data: &[u8], term: &str, doc_id: u64) -> Result<Option<Posting>> {
    let parsed = ParsedDictionary::parse(data)?;
    let Some(entry) = parsed.find_term(term) else {
        return Ok(None);
    };

    seek_doc(parsed.posting_payload(entry), doc_id)
}

/// Inspect the posting-list metadata for a single term.
pub fn inspect_term_postings(data: &[u8], term: &str) -> Result<Option<PostingListIndex>> {
    let parsed = ParsedDictionary::parse(data)?;
    let Some(entry) = parsed.find_term(term) else {
        return Ok(None);
    };

    crate::search::postings::inspect_postings(parsed.posting_payload(entry)).map(Some)
}

struct ParsedDictionary<'a> {
    index: TermDictionaryIndex,
    postings_blob: &'a [u8],
}

impl<'a> ParsedDictionary<'a> {
    fn parse(data: &'a [u8]) -> Result<Self> {
        if data.len() < HEADER_LEN {
            return Err(CompactError::InvalidInput(
                "term dictionary payload is truncated",
            ));
        }

        if data[..4] != MAGIC {
            return Err(CompactError::InvalidInput(
                "term dictionary has invalid magic",
            ));
        }

        if data[4] != VERSION {
            return Err(CompactError::Unsupported("term dictionary version"));
        }

        let expected_checksum = u32::from_le_bytes(
            data[5..9]
                .try_into()
                .expect("term dictionary checksum slice has fixed length"),
        );
        let body = &data[HEADER_LEN..];
        if checksum32(body) != expected_checksum {
            return Err(CompactError::InvalidInput(
                "term dictionary checksum mismatch",
            ));
        }

        let mut cursor = 0usize;
        let term_count = varint::read_u64(body, &mut cursor)?;
        let dictionary_len = varint::read_u64(body, &mut cursor)?;
        let postings_len = varint::read_u64(body, &mut cursor)?;
        let dictionary = take_section(
            body,
            &mut cursor,
            dictionary_len,
            "term dictionary metadata is truncated",
        )?;
        let postings_blob = take_section(
            body,
            &mut cursor,
            postings_len,
            "term dictionary postings are truncated",
        )?;

        if cursor != body.len() {
            return Err(CompactError::InvalidInput(
                "term dictionary has trailing bytes",
            ));
        }

        let entries = decode_entries(dictionary, term_count, postings_len)?;

        Ok(Self {
            index: TermDictionaryIndex {
                term_count,
                postings_bytes: postings_len,
                entries,
            },
            postings_blob,
        })
    }

    fn find_term(&self, term: &str) -> Option<&TermIndexEntry> {
        self.index
            .entries
            .binary_search_by(|entry| entry.term.as_str().cmp(term))
            .ok()
            .and_then(|idx| self.index.entries.get(idx))
    }

    fn posting_payload(&self, entry: &TermIndexEntry) -> &'a [u8] {
        let start = entry.postings_offset as usize;
        let end = start + entry.postings_len as usize;

        &self.postings_blob[start..end]
    }
}

fn validate_terms(entries: &[TermPostingList]) -> Result<()> {
    let mut previous = None;

    for entry in entries {
        if entry.term.is_empty() {
            return Err(CompactError::InvalidInput(
                "term dictionary term must be non-empty",
            ));
        }

        if let Some(previous_term) = previous
            && entry.term.as_str() <= previous_term
        {
            return Err(CompactError::InvalidInput(
                "term dictionary terms must be strictly increasing",
            ));
        }

        previous = Some(entry.term.as_str());
    }

    Ok(())
}

fn decode_entries(data: &[u8], term_count: u64, postings_len: u64) -> Result<Vec<TermIndexEntry>> {
    let capacity = usize::try_from(term_count)
        .map_err(|_| CompactError::InvalidInput("term dictionary count is too large"))?;
    let mut entries = Vec::with_capacity(capacity);
    let mut cursor = 0usize;
    let mut previous = None;

    for _ in 0..term_count {
        let term_len = varint::read_u64(data, &mut cursor)?;
        let term_bytes = take_section(
            data,
            &mut cursor,
            term_len,
            "term dictionary term is truncated",
        )?;
        let term = std::str::from_utf8(term_bytes)
            .map_err(|_| CompactError::InvalidInput("term dictionary term is not utf-8"))?
            .to_owned();
        let postings_offset = varint::read_u64(data, &mut cursor)?;
        let postings_len_for_term = varint::read_u64(data, &mut cursor)?;
        let doc_count = varint::read_u64(data, &mut cursor)?;

        if term.is_empty() {
            return Err(CompactError::InvalidInput(
                "term dictionary term must be non-empty",
            ));
        }

        if let Some(previous_term) = previous.as_ref()
            && term <= *previous_term
        {
            return Err(CompactError::InvalidInput(
                "term dictionary terms must be strictly increasing",
            ));
        }

        let end = postings_offset.checked_add(postings_len_for_term).ok_or(
            CompactError::InvalidInput("term dictionary postings range overflow"),
        )?;
        if end > postings_len {
            return Err(CompactError::InvalidInput(
                "term dictionary postings range out of bounds",
            ));
        }

        previous = Some(term.clone());
        entries.push(TermIndexEntry {
            term,
            postings_offset,
            postings_len: postings_len_for_term,
            doc_count,
        });
    }

    if cursor != data.len() {
        return Err(CompactError::InvalidInput(
            "term dictionary metadata has trailing bytes",
        ));
    }

    validate_posting_ranges(&entries)?;

    Ok(entries)
}

fn validate_posting_ranges(entries: &[TermIndexEntry]) -> Result<()> {
    let mut expected_offset = 0u64;

    for entry in entries {
        if entry.postings_offset != expected_offset {
            return Err(CompactError::InvalidInput(
                "term dictionary postings ranges must be contiguous",
            ));
        }

        expected_offset =
            expected_offset
                .checked_add(entry.postings_len)
                .ok_or(CompactError::InvalidInput(
                    "term dictionary postings range overflow",
                ))?;
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
        .map_err(|_| CompactError::InvalidInput("term dictionary section length is too large"))?;
    let end = cursor.checked_add(len).ok_or(CompactError::InvalidInput(
        "term dictionary section length overflow",
    ))?;
    let section = data
        .get(*cursor..end)
        .ok_or(CompactError::InvalidInput(message))?;
    *cursor = end;

    Ok(section)
}

#[cfg(test)]
mod tests {
    use super::{
        TermPostingList, encode_dictionary, inspect_dictionary, inspect_term_postings, lookup_term,
        seek_term_doc,
    };
    use crate::{CompactError, search::postings::Posting};

    fn entries() -> Vec<TermPostingList> {
        vec![
            TermPostingList {
                term: "alpha".to_owned(),
                postings: vec![
                    Posting {
                        doc_id: 1,
                        positions: vec![0, 3],
                    },
                    Posting {
                        doc_id: 10,
                        positions: vec![1],
                    },
                ],
            },
            TermPostingList {
                term: "beta".to_owned(),
                postings: vec![Posting {
                    doc_id: 4,
                    positions: vec![2, 9],
                }],
            },
            TermPostingList {
                term: "zulu".to_owned(),
                postings: vec![
                    Posting {
                        doc_id: 2,
                        positions: Vec::new(),
                    },
                    Posting {
                        doc_id: 7,
                        positions: vec![5],
                    },
                    Posting {
                        doc_id: 20,
                        positions: vec![8, 13],
                    },
                ],
            },
        ]
    }

    #[test]
    fn dictionary_roundtrips_one_term_without_decoding_others() {
        let encoded = encode_dictionary(&entries(), 2).unwrap();

        assert_eq!(
            lookup_term(&encoded, "beta").unwrap(),
            Some(vec![Posting {
                doc_id: 4,
                positions: vec![2, 9],
            }])
        );
        assert_eq!(lookup_term(&encoded, "missing").unwrap(), None);
    }

    #[test]
    fn dictionary_inspect_returns_sorted_metadata() {
        let encoded = encode_dictionary(&entries(), 2).unwrap();
        let index = inspect_dictionary(&encoded).unwrap();

        assert_eq!(index.term_count, 3);
        assert_eq!(index.entries[0].term, "alpha");
        assert_eq!(index.entries[0].doc_count, 2);
        assert_eq!(index.entries[1].term, "beta");
        assert_eq!(index.entries[1].doc_count, 1);
        assert_eq!(index.entries[2].term, "zulu");
        assert_eq!(index.entries[2].doc_count, 3);
        assert!(index.postings_bytes > 0);
    }

    #[test]
    fn dictionary_can_seek_doc_inside_one_term() {
        let encoded = encode_dictionary(&entries(), 2).unwrap();

        assert_eq!(
            seek_term_doc(&encoded, "zulu", 20).unwrap(),
            Some(Posting {
                doc_id: 20,
                positions: vec![8, 13],
            })
        );
        assert_eq!(seek_term_doc(&encoded, "zulu", 19).unwrap(), None);
        assert_eq!(seek_term_doc(&encoded, "missing", 20).unwrap(), None);
    }

    #[test]
    fn dictionary_can_inspect_nested_posting_metadata() {
        let encoded = encode_dictionary(&entries(), 2).unwrap();
        let posting_index = inspect_term_postings(&encoded, "zulu").unwrap().unwrap();

        assert_eq!(posting_index.doc_count, 3);
        assert_eq!(posting_index.skip_entries.len(), 2);
    }

    #[test]
    fn encoder_rejects_empty_unsorted_and_duplicate_terms() {
        let err = encode_dictionary(
            &[TermPostingList {
                term: String::new(),
                postings: Vec::new(),
            }],
            2,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            CompactError::InvalidInput("term dictionary term must be non-empty")
        ));

        let err = encode_dictionary(
            &[
                TermPostingList {
                    term: "b".to_owned(),
                    postings: Vec::new(),
                },
                TermPostingList {
                    term: "a".to_owned(),
                    postings: Vec::new(),
                },
            ],
            2,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            CompactError::InvalidInput("term dictionary terms must be strictly increasing")
        ));
    }

    #[test]
    fn decoder_rejects_bad_magic_and_checksum() {
        let mut encoded = encode_dictionary(&entries(), 2).unwrap();
        encoded[0] = b'X';
        let err = inspect_dictionary(&encoded).unwrap_err();
        assert!(matches!(
            err,
            CompactError::InvalidInput("term dictionary has invalid magic")
        ));

        let mut encoded = encode_dictionary(&entries(), 2).unwrap();
        let last = encoded.len() - 1;
        encoded[last] ^= 0x01;
        let err = inspect_dictionary(&encoded).unwrap_err();
        assert!(matches!(
            err,
            CompactError::InvalidInput("term dictionary checksum mismatch")
        ));
    }
}
