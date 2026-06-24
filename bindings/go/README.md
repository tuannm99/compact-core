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
func Version() string
func EncodeBytesRLE(input []byte) ([]byte, error)
func DecodeBytesRLE(input []byte) ([]byte, error)
```

Byte buffers returned by Rust are copied into Go memory and released with
`compact_buffer_free` before the Go function returns.
