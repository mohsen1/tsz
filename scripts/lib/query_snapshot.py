"""Shared helpers for querying offline JSON snapshot files.

Used by scripts/emit/query-emit.py and scripts/fourslash/query-fourslash.py.
"""

import json
import sys
from collections import Counter
from pathlib import Path


def load_snapshot(path: Path, run_hint: str = "Run the test suite with --json-out to generate it.") -> dict:
    """Load a JSON snapshot file, printing a helpful error and exiting if missing."""
    if not path.exists():
        print(f"Error: {path} not found.")
        print(run_hint)
        sys.exit(1)
    with open(path) as f:
        return json.load(f)


def print_top_counter(counter: Counter, top: int = 20) -> None:
    """Print the top-N entries from a Counter, right-aligned count followed by the key."""
    for msg, count in counter.most_common(top):
        print(f"  {count:>4d}  {msg}")


def filter_by_name(items: list, pattern: str, name_key: str = "name") -> list:
    """Return items whose name_key contains pattern (case-insensitive)."""
    lower = pattern.lower()
    return [r for r in items if lower in r.get(name_key, "").lower()]


def print_truncated_more(items, top: int, indent: str = "  ") -> None:
    """Print the standard ``... and N more`` truncation tail.

    Emits ``{indent}... and {len(items) - top} more`` when ``len(items) > top``,
    otherwise no output. ``items`` may be any sized iterable (list, tuple, etc.).
    Centralizes the truncation pattern repeated across the offline query scripts
    in ``scripts/fourslash/query-fourslash.py`` and ``scripts/emit/query-emit.py``.
    """
    remaining = len(items) - top
    if remaining > 0:
        print(f"{indent}... and {remaining} more")
