# v0.8 Parallel Compression Engine

v0.8 scales CMP2 block compression and decompression across CPU cores without
changing the file format. Blocks remain independently decodable, checksummed,
and indexed by the existing `IDX1` footer.

Read in this order:

1. [Scope and decisions](01-scope-and-decisions.md)
2. [Scheduler contract](02-scheduler-contract.md)
3. [Implementation phases](03-implementation-plan.md)
