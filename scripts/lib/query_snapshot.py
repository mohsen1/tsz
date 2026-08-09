"""Shared helpers for querying offline JSON snapshot files.

Used by scripts/emit/query-emit.py and scripts/fourslash/query-fourslash.py.
"""

import json
import sys
from collections import Counter
from pathlib import Path


# Fourslash outcome taxonomy (issue #17010). The runner classifies every test
# into exactly one status; these group which statuses/buckets count as passing.
# Offline consumers import them instead of each re-encoding the split, so adding
# or reclassifying a bucket is a one-line change here rather than in three
# scripts. A completed-but-slow test passed its assertions — the slowness is a
# harness perf signal, not a correctness failure — so `slow` is a passing bucket.
FOURSLASH_PASSING_BUCKETS = ("pass", "slow")
FOURSLASH_NON_PASSING_BUCKETS = ("fail", "timeout", "unrun")
FOURSLASH_NON_PASSING_STATUSES = frozenset(FOURSLASH_NON_PASSING_BUCKETS)


def fourslash_bucket_counts(data: dict) -> tuple:
    """``(passed, total)`` from a compact snapshot that lacks a ``summary`` block.

    Slow tests count as passing; only fail/timeout/unrun are non-passing.
    Tolerates older snapshots missing the slow/timeout/unrun buckets —
    ``data.get(b) or []`` treats an absent bucket as empty.
    """
    passed = sum(len(data.get(b) or []) for b in FOURSLASH_PASSING_BUCKETS)
    non_passing = sum(len(data.get(b) or []) for b in FOURSLASH_NON_PASSING_BUCKETS)
    return passed, passed + non_passing


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
