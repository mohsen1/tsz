#!/usr/bin/env python3
"""Fail when a PR adds entries to the accepted-conformance-regression ledger.

The accepted-regression file records conformance tests that are temporarily
allowed to fail.  Its budget is monotonically non-increasing on main: a PR
may remove entries (progress) but must never add new ones (regression debt).

Usage:
    python3 check-accepted-regression-growth.py --base-ref origin/main
    python3 check-accepted-regression-growth.py --base-ref origin/main --head-ref HEAD
"""

from __future__ import annotations

import argparse
import subprocess
import sys

REGRESSIONS_PATH = "scripts/conformance/conformance-accepted-regressions.txt"


def _read_ref_text(ref: str, path: str) -> str | None:
    """Return the text of path at ref, or None if the ref/path is unavailable."""
    try:
        return subprocess.check_output(
            ["git", "show", f"{ref}:{path}"],
            text=True,
            stderr=subprocess.PIPE,
        )
    except subprocess.CalledProcessError:
        return None


def load_entries(ref: str, path: str = REGRESSIONS_PATH) -> frozenset[str] | None:
    """Return normalized non-comment entries at ref, or None if the ref is unavailable."""
    raw = _read_ref_text(ref, path)
    if raw is None:
        return None
    entries: set[str] = set()
    for line in raw.splitlines():
        stripped = line.strip()
        if stripped and not stripped.startswith("#"):
            entries.add(stripped)
    return frozenset(entries)


def check_growth(
    base_entries: frozenset[str],
    head_entries: frozenset[str],
) -> tuple[frozenset[str], frozenset[str]]:
    """Return (added, removed) entry sets relative to base."""
    return head_entries - base_entries, base_entries - head_entries


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Fail if the accepted-regression ledger grew between a base ref and HEAD. "
            "Removals are always allowed; additions are never allowed."
        )
    )
    parser.add_argument(
        "--base-ref",
        required=True,
        help="Base git ref to compare against (e.g. origin/main).",
    )
    parser.add_argument(
        "--head-ref",
        default="HEAD",
        help="Head git ref to inspect (default: HEAD).",
    )
    parser.add_argument(
        "--path",
        default=REGRESSIONS_PATH,
        help=f"Path to accepted-regression file (default: {REGRESSIONS_PATH}).",
    )
    parser.add_argument(
        "--allow-unavailable-base",
        action="store_true",
        help=(
            "Emit a warning and exit 0 when the base ref cannot be read "
            "(e.g. shallow clones on local runs)."
        ),
    )
    args = parser.parse_args(argv)

    base_entries = load_entries(args.base_ref, args.path)
    if base_entries is None:
        msg = (
            f"could not read {args.path} from {args.base_ref!r}; "
            "accepted-regression growth check skipped."
        )
        if args.allow_unavailable_base:
            print(f"warning: {msg}", file=sys.stderr)
            return 0
        print(f"error: {msg}", file=sys.stderr)
        return 1

    head_entries = load_entries(args.head_ref, args.path)
    if head_entries is None:
        print(
            f"error: could not read {args.path} from {args.head_ref!r}",
            file=sys.stderr,
        )
        return 1

    added, removed = check_growth(base_entries, head_entries)

    print(
        f"Accepted-regression ledger: "
        f"{len(base_entries)} entries at {args.base_ref!r}, "
        f"{len(head_entries)} at {args.head_ref!r}."
    )
    if removed:
        print(f"Removed entries ({len(removed)}):")
        for entry in sorted(removed):
            print(f"  - {entry}")
    if added:
        print(f"Added entries ({len(added)}) — this is not allowed:")
        for entry in sorted(added):
            print(f"  + {entry}")
        print()
        print(
            "::error::The accepted-regression ledger must not grow. "
            "File a parity issue and track the regression there instead of "
            "adding a new entry to conformance-accepted-regressions.txt."
        )
        return 1

    print("Accepted-regression growth gate passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
