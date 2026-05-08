# Go binding

Planned API:

```go
package compact

func EncodeFile(input, schema, output string) error
func DecodeFile(input, output string) error
```

This directory will wrap the `compact-ffi` C ABI in Phase 8.
