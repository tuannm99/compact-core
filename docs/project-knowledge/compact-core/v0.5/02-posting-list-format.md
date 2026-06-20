# Posting-List Format

The phase-1 payload is a single checked posting list:

```text
[magic:4 = PST1]
[version:1 = 1]
[checksum:4 little-endian over body]
[body]
```

The body starts with six canonical varints:

```text
doc_count
skip_step
doc_ids_len
frequencies_len
positions_len
skip_table_len
```

Then four byte sections follow:

```text
doc_ids
frequencies
positions
skip_table
```

## DocID Section

DocIDs are delta encoded from the previous docID, then varint encoded.

```text
doc_ids: 10, 13, 20
deltas:  10,  3,  7
```

DocIDs must be strictly increasing after decode.

## Frequencies Section

The frequencies section stores one varint per posting:

```text
frequency = number of positions for this term in this document
```

This lets the decoder know how many position deltas to read for each posting.

## Positions Section

Positions are delta encoded per document. The first position is delta encoded
from zero.

```text
positions: 2, 9, 12
deltas:    2, 7,  3
```

Positions must be strictly increasing inside each posting.

## Skip Table

Each skip entry stores six varints:

```text
ordinal
first_doc_id
previous_doc_id
doc_ids_offset
frequencies_offset
positions_offset
```

`ordinal` is the posting ordinal where the skip block starts. Offsets are byte
offsets into their corresponding sections. `previous_doc_id` lets a reader
resume delta decoding from the skip boundary.
