from __future__ import annotations

import unittest

import compact


class CompactBindingTests(unittest.TestCase):
    def test_byte_roundtrip(self) -> None:
        data = b"aaaaabbbbbbbbbcccccccc"

        encoded = compact.encode_bytes_rle(data)
        decoded = compact.decode_bytes_rle(encoded)

        self.assertEqual(decoded, data)

    def test_version_is_available(self) -> None:
        self.assertTrue(compact.version())


if __name__ == "__main__":
    unittest.main()
