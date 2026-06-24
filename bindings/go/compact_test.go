package compact

import "testing"

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
