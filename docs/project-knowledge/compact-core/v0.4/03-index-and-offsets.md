# Index and Offsets

## Row Group Index

Each row group index entry stores:

- `row_group_index`: contiguous zero-based index.
- `first_row_index`: logical row where the group starts.
- `row_count`: number of rows in the group.
- `row_group_offset`: byte offset from file start.
- `row_group_len`: byte length.
- `columns`: column index entries inside this row group.

Row groups must be sorted and logically contiguous. This enables binary search
by row number without an auxiliary sparse index.

## Column Index

Each column index entry stores:

- Column name.
- Metadata offset and length.
- Payload offset and length.
- Value count and null count.
- Serialized statistics metadata.

Both metadata and payload ranges must be inside the row group range. This is a
security boundary: a corrupt footer must not be able to point a projection read
outside the row group it claims to describe.

## Offset Rules

- Offsets are absolute file offsets.
- Length arithmetic must use checked addition.
- Footer ranges must end exactly before the trailer.
- Row group payload ranges must end before the footer starts.
- Duplicate column names inside one row group are invalid.
