#!/usr/bin/env python3
"""Generate deterministic schema-backed JSONL for compact v0.2 validation."""

from __future__ import annotations

import argparse
from pathlib import Path


LEVELS = ("INFO", "WARN", "ERROR")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate JSONL and schema fixtures for compact streaming validation."
    )
    parser.add_argument(
        "--rows",
        type=int,
        required=True,
        help="Number of JSONL rows to generate.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        required=True,
        help="Path to write the generated JSONL file.",
    )
    parser.add_argument(
        "--schema",
        type=Path,
        required=True,
        help="Path to write the matching compact schema YAML.",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    if args.rows < 0:
        raise SystemExit("--rows must be greater than or equal to zero")

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.schema.parent.mkdir(parents=True, exist_ok=True)

    with args.output.open("w", encoding="utf-8") as out:
        for index in range(args.rows):
            level = LEVELS[index % len(LEVELS)]
            ts = 1_700_000_000 + index
            out.write(f'{{"ts":{ts},"level":"{level}"}}\n')

    args.schema.write_text(
        "\n".join(
            [
                "columns:",
                "  - name: ts",
                "    type: u64",
                "    codec: delta_varint_u64",
                "  - name: level",
                "    type: string",
                "    codec: rle",
                "",
            ]
        ),
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
