//! Query helpers over v0.5 search dictionary blocks.
//!
//! These helpers are intentionally small. They prove the compressed structures
//! can serve search-style access patterns without becoming a search engine.

use crate::{
    Result,
    search::{
        dictionary::{lookup_term, seek_term_doc},
        postings::Posting,
    },
};

/// One scored document for simple term-frequency ranking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopKHit {
    pub doc_id: u64,
    pub score: u64,
}

/// Return the docIDs for one term.
pub fn term_doc_ids(dictionary: &[u8], term: &str) -> Result<Vec<u64>> {
    Ok(lookup_term(dictionary, term)?
        .unwrap_or_default()
        .into_iter()
        .map(|posting| posting.doc_id)
        .collect())
}

/// Return docIDs that appear in every requested term.
pub fn and_doc_ids(dictionary: &[u8], terms: &[&str]) -> Result<Vec<u64>> {
    if terms.is_empty() {
        return Ok(Vec::new());
    }

    let mut lists = Vec::with_capacity(terms.len());
    for term in terms {
        let postings = lookup_term(dictionary, term)?.unwrap_or_default();
        if postings.is_empty() {
            return Ok(Vec::new());
        }
        lists.push(postings);
    }

    lists.sort_by_key(Vec::len);
    let mut result = Vec::new();

    for candidate in &lists[0] {
        if lists[1..].iter().all(|list| {
            list.binary_search_by_key(&candidate.doc_id, |p| p.doc_id)
                .is_ok()
        }) {
            result.push(candidate.doc_id);
        }
    }

    Ok(result)
}

/// Check whether two terms occur as an adjacent phrase in one document.
pub fn has_adjacent_phrase(
    dictionary: &[u8],
    first_term: &str,
    second_term: &str,
    doc_id: u64,
) -> Result<bool> {
    let Some(first) = lookup_term(dictionary, first_term)?
        .unwrap_or_default()
        .into_iter()
        .find(|posting| posting.doc_id == doc_id)
    else {
        return Ok(false);
    };
    let Some(second) = lookup_term(dictionary, second_term)?
        .unwrap_or_default()
        .into_iter()
        .find(|posting| posting.doc_id == doc_id)
    else {
        return Ok(false);
    };

    Ok(has_position_gap(&first, &second, 1))
}

/// Rank documents by summed term frequency across the requested terms.
pub fn top_k_by_term_frequency(
    dictionary: &[u8],
    terms: &[&str],
    k: usize,
) -> Result<Vec<TopKHit>> {
    if k == 0 || terms.is_empty() {
        return Ok(Vec::new());
    }

    let mut scores = Vec::<TopKHit>::new();

    for term in terms {
        for posting in lookup_term(dictionary, term)?.unwrap_or_default() {
            let score = posting.positions.len() as u64;
            if score == 0 {
                continue;
            }

            if let Some(hit) = scores.iter_mut().find(|hit| hit.doc_id == posting.doc_id) {
                hit.score += score;
            } else {
                scores.push(TopKHit {
                    doc_id: posting.doc_id,
                    score,
                });
            }
        }
    }

    scores.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.doc_id.cmp(&right.doc_id))
    });
    scores.truncate(k);

    Ok(scores)
}

/// Use a dictionary entry and posting skip table to check one document quickly.
pub fn term_contains_doc(dictionary: &[u8], term: &str, doc_id: u64) -> Result<bool> {
    Ok(seek_term_doc(dictionary, term, doc_id)?.is_some())
}

fn has_position_gap(first: &Posting, second: &Posting, gap: u64) -> bool {
    first.positions.iter().any(|position| {
        position
            .checked_add(gap)
            .is_some_and(|next| second.positions.binary_search(&next).is_ok())
    })
}

#[cfg(test)]
mod tests {
    use super::{
        TopKHit, and_doc_ids, has_adjacent_phrase, term_contains_doc, term_doc_ids,
        top_k_by_term_frequency,
    };
    use crate::search::{
        dictionary::{TermPostingList, encode_dictionary},
        postings::Posting,
    };

    fn dictionary() -> Vec<u8> {
        encode_dictionary(
            &[
                TermPostingList {
                    term: "brown".to_owned(),
                    postings: vec![
                        Posting {
                            doc_id: 1,
                            positions: vec![1],
                        },
                        Posting {
                            doc_id: 3,
                            positions: vec![4, 8],
                        },
                    ],
                },
                TermPostingList {
                    term: "fox".to_owned(),
                    postings: vec![
                        Posting {
                            doc_id: 1,
                            positions: vec![2, 9],
                        },
                        Posting {
                            doc_id: 2,
                            positions: vec![1],
                        },
                        Posting {
                            doc_id: 3,
                            positions: vec![5],
                        },
                    ],
                },
                TermPostingList {
                    term: "quick".to_owned(),
                    postings: vec![Posting {
                        doc_id: 1,
                        positions: vec![0],
                    }],
                },
            ],
            2,
        )
        .unwrap()
    }

    #[test]
    fn term_doc_ids_returns_sorted_matches() {
        assert_eq!(term_doc_ids(&dictionary(), "fox").unwrap(), vec![1, 2, 3]);
        assert!(term_doc_ids(&dictionary(), "missing").unwrap().is_empty());
    }

    #[test]
    fn and_doc_ids_intersects_shortest_list_first() {
        assert_eq!(
            and_doc_ids(&dictionary(), &["brown", "fox"]).unwrap(),
            vec![1, 3]
        );
        assert_eq!(
            and_doc_ids(&dictionary(), &["quick", "fox", "brown"]).unwrap(),
            vec![1]
        );
        assert!(
            and_doc_ids(&dictionary(), &["quick", "missing"])
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn adjacent_phrase_uses_positions() {
        assert!(has_adjacent_phrase(&dictionary(), "quick", "brown", 1).unwrap());
        assert!(has_adjacent_phrase(&dictionary(), "brown", "fox", 3).unwrap());
        assert!(!has_adjacent_phrase(&dictionary(), "fox", "brown", 1).unwrap());
    }

    #[test]
    fn top_k_uses_summed_term_frequency_then_doc_id() {
        assert_eq!(
            top_k_by_term_frequency(&dictionary(), &["brown", "fox"], 2).unwrap(),
            vec![
                TopKHit {
                    doc_id: 1,
                    score: 3
                },
                TopKHit {
                    doc_id: 3,
                    score: 3
                },
            ]
        );
    }

    #[test]
    fn term_contains_doc_uses_posting_seek() {
        assert!(term_contains_doc(&dictionary(), "fox", 3).unwrap());
        assert!(!term_contains_doc(&dictionary(), "fox", 4).unwrap());
        assert!(!term_contains_doc(&dictionary(), "missing", 3).unwrap());
    }
}
