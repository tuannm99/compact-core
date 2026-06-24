package compact

/*
#cgo LDFLAGS: -lcompact_ffi
#include <stdlib.h>

int compact_encode_file(const char *input_path, const char *schema_path, const char *output_path);
int compact_decode_file_with_schema(const char *input_path, const char *schema_path, const char *output_path);
const char *compact_version(void);
const char *compact_status_message(int status);

typedef struct CompactBuffer {
    unsigned char *ptr;
    size_t len;
    size_t capacity;
} CompactBuffer;

int compact_encode_bytes_rle(const unsigned char *input_ptr, size_t input_len, CompactBuffer *output);
int compact_decode_bytes_rle(const unsigned char *input_ptr, size_t input_len, CompactBuffer *output);
void compact_buffer_free(CompactBuffer *buffer);
*/
import "C"

import (
	"fmt"
	"unsafe"
)

const (
	statusOK            = 0
	statusNullPtr       = 1
	statusUnimplemented = 2
	statusIO            = 3
	statusInvalidInput  = 4
)

func EncodeFile(inputPath, schemaPath, outputPath string) error {
	input := C.CString(inputPath)
	schema := C.CString(schemaPath)
	output := C.CString(outputPath)
	defer C.free(unsafe.Pointer(input))
	defer C.free(unsafe.Pointer(schema))
	defer C.free(unsafe.Pointer(output))

	return statusToError(int(C.compact_encode_file(input, schema, output)))
}

func DecodeFile(inputPath, schemaPath, outputPath string) error {
	input := C.CString(inputPath)
	schema := C.CString(schemaPath)
	output := C.CString(outputPath)
	defer C.free(unsafe.Pointer(input))
	defer C.free(unsafe.Pointer(schema))
	defer C.free(unsafe.Pointer(output))

	return statusToError(int(C.compact_decode_file_with_schema(input, schema, output)))
}

func Version() string {
	return C.GoString(C.compact_version())
}

func EncodeBytesRLE(input []byte) ([]byte, error) {
	return withBytes(input, func(inputPtr *C.uchar, inputLen C.size_t, output *C.CompactBuffer) C.int {
		return C.compact_encode_bytes_rle(inputPtr, inputLen, output)
	})
}

func DecodeBytesRLE(input []byte) ([]byte, error) {
	return withBytes(input, func(inputPtr *C.uchar, inputLen C.size_t, output *C.CompactBuffer) C.int {
		return C.compact_decode_bytes_rle(inputPtr, inputLen, output)
	})
}

func statusToError(status int) error {
	switch status {
	case statusOK:
		return nil
	case statusNullPtr:
		return fmt.Errorf("compact: null pointer")
	case statusUnimplemented:
		return fmt.Errorf("compact: unimplemented")
	case statusIO:
		return fmt.Errorf("compact: i/o error")
	case statusInvalidInput:
		return fmt.Errorf("compact: invalid input")
	default:
		return fmt.Errorf("compact: unknown status %d", status)
	}
}

func withBytes(input []byte, call func(*C.uchar, C.size_t, *C.CompactBuffer) C.int) ([]byte, error) {
	var inputPtr *C.uchar
	if len(input) > 0 {
		inputPtr = (*C.uchar)(unsafe.Pointer(&input[0]))
	}

	var output C.CompactBuffer
	status := int(call(inputPtr, C.size_t(len(input)), &output))
	if err := statusToError(status); err != nil {
		return nil, err
	}
	defer C.compact_buffer_free(&output)

	return C.GoBytes(unsafe.Pointer(output.ptr), C.int(output.len)), nil
}
