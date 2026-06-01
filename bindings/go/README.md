# Go binding

Minimal cgo wrapper over `compact-ffi`.

Build the Rust FFI library first:

```sh
cargo build -p compact-ffi
```

Then link Go with the directory containing `libcompact_ffi`.

```go
package compact

func EncodeFile(input, schema, output string) error
func DecodeFile(input, schema, output string) error
```
