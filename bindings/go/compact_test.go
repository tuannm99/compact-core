package compact

import (
	"bytes"
	"os"
	"testing"
)

func TestByteRoundtrip(t *testing.T) {
	input := []byte("aaaaabbbbbbbbbcccccccc")

	encoded, err := EncodeBytesRLE(input)
	if err != nil {
		t.Fatalf("EncodeBytesRLE failed: %v", err)
	}

	decoded, err := DecodeBytesRLE(encoded)
	if err != nil {
		t.Fatalf("DecodeBytesRLE failed: %v", err)
	}

	if string(decoded) != string(input) {
		t.Fatalf("decoded mismatch: got %q want %q", decoded, input)
	}
}

func TestVersion(t *testing.T) {
	if Version() == "" {
		t.Fatal("empty version")
	}
}

func TestValidatePathsRejectsEmbeddedNUL(t *testing.T) {
	if err := validatePaths("input\x00ignored", "schema.yml", "output.cmp"); err == nil {
		t.Fatal("expected embedded NUL path to be rejected")
	}
}

func TestCopyOutputRejectsCIntOverflow(t *testing.T) {
	if _, err := copyOutput(nil, maxCGoBytes+1); err == nil {
		t.Fatal("expected oversized output to be rejected")
	}
}

func TestCrossLanguageFileFixture(t *testing.T) {
	input := os.Getenv("COMPACT_CROSS_INPUT")
	schema := os.Getenv("COMPACT_CROSS_SCHEMA")
	encoded := os.Getenv("COMPACT_CROSS_ENCODED")
	goEncoded := os.Getenv("COMPACT_CROSS_GO_ENCODED")
	if input == "" || schema == "" || encoded == "" || goEncoded == "" {
		t.Skip("cross-language fixture environment is not configured")
	}

	decoded := goEncoded + ".decoded"
	if err := DecodeFile(encoded, schema, decoded); err != nil {
		t.Fatalf("DecodeFile failed: %v", err)
	}
	want, err := os.ReadFile(input)
	if err != nil {
		t.Fatal(err)
	}
	got, err := os.ReadFile(decoded)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(got, want) {
		t.Fatal("cross-language decoded file mismatch")
	}
	if err := EncodeFile(input, schema, goEncoded); err != nil {
		t.Fatalf("EncodeFile failed: %v", err)
	}
}
