"""Shared helpers for conformance offline-query scripts.

Used by `scripts/conformance/query-conformance.py`,
`scripts/conformance/classify-render-corpus.py`, and
`scripts/conformance/analyze-conformance.py`.

All helpers operate on the compact failure record format produced by
`scripts/conformance/build-snapshot-detail.py`:

    failures[test_path] = {
        "e": [code, ...],   # expected codes (omitted when empty)
        "a": [code, ...],   # actual codes (omitted when empty)
        "m": [code, ...],   # missing codes (omitted when empty)
        "x": [code, ...],   # extra codes (omitted when empty)
    }
"""

from __future__ import annotations

from collections import Counter
from pathlib import Path

from lib.query_snapshot import load_snapshot

# Default location of the conformance detail snapshot. Resolved relative to the
# repository so script callers do not have to pass an explicit path.
DEFAULT_DETAIL_PATH = Path(__file__).resolve().parent.parent / "conformance" / "conformance-detail.json"

DEFAULT_DETAIL_HINT = "Run: ./scripts/conformance/conformance.sh snapshot"


def load_detail(detail_path: Path | None = None, hint: str | None = None) -> dict:
    """Load `conformance-detail.json` (or a caller-provided path).

    Reuses :func:`scripts.lib.query_snapshot.load_snapshot` so the
    "missing file -> exit with hint" behavior matches sibling query tools.
    """
    path = Path(detail_path) if detail_path is not None else DEFAULT_DETAIL_PATH
    return load_snapshot(path, hint or DEFAULT_DETAIL_HINT)


def basename(path: str) -> str:
    """Return the final path segment, or ``path`` itself when there is no `/`."""
    return path.rsplit("/", 1)[-1] if "/" in path else path


def code_counts(codes) -> Counter:
    """Return a :class:`Counter` of the given iterable of error codes.

    Thin wrapper kept for API parity across the conformance query scripts.
    """
    return Counter(codes)


def is_fingerprint_only(failure: dict) -> bool:
    """True when expected/actual code multisets are equal but the test failed.

    Fingerprint-only failures emit the right error codes with the right
    multiplicities but mismatch on message text, position, or some other
    fingerprint detail.
    """
    expected = failure.get("e", [])
    actual = failure.get("a", [])
    if not expected or not actual:
        return False
    return code_counts(expected) == code_counts(actual)


def is_same_code_count_drift(failure: dict) -> bool:
    """True when expected/actual share the same code set with different counts.

    Equivalent to "compact code lists hide a count-only mismatch": both sides
    contain at least one of the same codes, but the multiplicities differ.
    """
    expected = failure.get("e", [])
    actual = failure.get("a", [])
    if not expected or not actual:
        return False
    expected_counts = code_counts(expected)
    actual_counts = code_counts(actual)
    return expected_counts != actual_counts and set(expected_counts) == set(actual_counts)
