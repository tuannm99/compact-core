package compact

/*
#cgo LDFLAGS: -lcompact_ffi
#include <stdlib.h>

int compact_encode_file(const char *input_path, const char *schema_path, const char *output_path);
int compact_decode_file_with_schema(const char *input_path, const char *schema_path, const char *output_path);
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
