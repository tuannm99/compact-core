from __future__ import annotations

import unittest
import tempfile
from pathlib import Path

import compact


class CompactBindingTests(unittest.TestCase):
    def test_byte_roundtrip(self) -> None:
        data = b"aaaaabbbbbbbbbcccccccc"

        encoded = compact.encode_bytes_rle(data)
        decoded = compact.decode_bytes_rle(encoded)

        self.assertEqual(decoded, data)

    def test_version_is_available(self) -> None:
        self.assertTrue(compact.version())

    def test_file_roundtrip_and_invalid_schema_status(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            input_path = root / "input.jsonl"
            schema_path = root / "schema.yml"
            encoded_path = root / "encoded.cmp"
            decoded_path = root / "decoded.jsonl"
            input_path.write_text('{"ts":1}\n', encoding="utf-8")
            schema_path.write_text(
                "columns:\n  - name: ts\n    type: u64\n    codec: delta_varint_u64\n",
                encoding="utf-8",
            )

            compact.encode_file(input_path, schema_path, encoded_path)
            compact.decode_file(encoded_path, schema_path, decoded_path)
            self.assertEqual(decoded_path.read_bytes(), input_path.read_bytes())

            schema_path.write_text("columns: [", encoding="utf-8")
            with self.assertRaises(compact.CompactError):
                compact.encode_file(input_path, schema_path, encoded_path)


if __name__ == "__main__":
    unittest.main()
