# Dictionary and Query API

v0.5 separates storage from query helpers:

- `search::postings` owns one compressed posting list.
- `search::dictionary` owns many sorted terms and their posting-list payloads.
- `search::query` owns small search-style operations over dictionary bytes.

This keeps the codec reusable. A future search engine can use the dictionary
block directly without depending on CLI code or a specific ranking model.

## Dictionary Block

The dictionary payload uses magic `TRM1`:

```text
[magic:4 = TRM1]
[version:1 = 1]
[checksum:4 little-endian over body]
[body]
```

The body starts with:

```text
term_count
dictionary_len
postings_blob_len
```

The dictionary section stores sorted term metadata:

```text
term_len
term_utf8_bytes
postings_offset
postings_len
doc_count
```

The postings blob stores concatenated `PST1` posting-list payloads.

## Lookup Complexity

Terms are sorted lexicographically. Term lookup is binary search:

```text
O(log T)
```

where `T` is the number of terms.

After finding a term, the reader slices exactly that term's posting-list
payload. It does not decode unrelated terms.

## Random Seek

`seek_term_doc(dictionary, term, doc_id)` performs:

```text
term binary search -> posting-list skip lookup -> local docID scan
```

The expected cost is:

```text
O(log T + log S + K)
```

where:

- `T` is term count.
- `S` is skip-entry count for the term.
- `K` is the number of postings scanned after the nearest skip entry.

Smaller skip steps reduce `K` but increase metadata size.

## Query Helpers

The current query helpers are intentionally narrow:

- `term_doc_ids`: return sorted docIDs for one term.
- `and_doc_ids`: intersect docIDs across terms.
- `has_adjacent_phrase`: check a two-term adjacent phrase in one document.
- `top_k_by_term_frequency`: rank documents by summed term frequency.
- `term_contains_doc`: test one `(term, doc_id)` pair using dictionary seek.

These APIs prove the compressed layout supports search access patterns. They
are not a full search engine.
