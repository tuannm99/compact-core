# Node binding

Dependency-free Node wrapper over the `compact` CLI. Set `COMPACT_BIN` when the
binary is not on `PATH`.

```ts
export function encodeFile(input: string, schema: string, output: string): void;
export function decodeFile(input: string, schema: string, output: string): void;
export function version(): string;
export function encodeBytesRle(input: Uint8Array): Uint8Array;
export function decodeBytesRle(input: Uint8Array): Uint8Array;
```

This wrapper shells out to the CLI rather than using a native addon. A future
native binding can wrap the C ABI directly and keep the same JavaScript API.
