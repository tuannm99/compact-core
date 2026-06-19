# Predicate Pushdown

Predicate pushdown means using statistics to skip row groups before decoding
payloads.

## Required Statistics

For each comparable column, persist enough metadata to answer simple predicates:

- Minimum value.
- Maximum value.
- Null count.
- Value count.

String statistics must be byte-stable and documented. Numeric statistics must
use the same logical type as the schema.

## Safe Pruning Rules

Pushdown may only skip a row group when metadata proves the predicate cannot
match. If statistics are missing, malformed, or unsupported, the row group must
be scanned.

Examples:

- `column = 5` can skip when `5 < min` or `5 > max`.
- `column > 10` can skip when `max <= 10`.
- `column IS NULL` can skip when `null_count == 0`.

The planner must prefer false negatives over false positives. It is acceptable
to scan extra row groups; it is not acceptable to skip matching rows.
