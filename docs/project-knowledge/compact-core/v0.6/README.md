# v0.6 Real-Time Streaming Integration

v0.6 adapts the existing streaming block engine for append-oriented systems.
The core rule is:

```text
an append stream must be recoverable to the last fully valid block
```

Read the documents in this order:

1. [Scope and decisions](01-scope-and-decisions.md)
2. [Append stream recovery](02-append-stream-recovery.md)
3. [Implementation phases](03-implementation-plan.md)
4. [Kafka-style example](../../../examples/v0.6-kafka-style.md)
