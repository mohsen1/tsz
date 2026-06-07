#!/usr/bin/env python3
"""Guard the accepted-conformance-regression ledger.

The accepted-regression file records conformance tests that are temporarily
allowed to fail.  Its budget is monotonically non-increasing on main: a PR
may remove entries (progress) but must never add new ones (regression debt).
The ledger must also stay canonical (no duplicate, non-normalized, or
malformed entries) so the visible counter and the CI aggregate matcher agree.

Usage:
    # Growth + integrity (default): additions are rejected, removals allowed.
    python3 check-accepted-regression-growth.py --base-ref origin/main
    python3 check-accepted-regression-growth.py --base-ref origin/main --head-ref HEAD

    # Integrity only: validate HEAD's ledger hygiene without a base ref.
    # Safe to run on any event (push, merge_group, ...).
    python3 check-accepted-regression-growth.py --integrity-only
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

from accepted_regressions import (  # noqa: E402  (path injected above)
    check_growth,
    check_integrity,
    entry_set,
)

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


def _report_integrity(ref_label: str, text: str) -> bool:
    """Print integrity problems for a ledger text. Return True when clean."""
    problems = check_integrity(text)
    if not problems:
        print(f"Accepted-regression ledger integrity passed ({ref_label}).")
        return True

    print(
        f"Accepted-regression ledger has {len(problems)} integrity problem(s) "
        f"({ref_label}):"
    )
    for problem in problems:
        print(f"  - {problem.format()}")
        print(f"::error::accepted-regression ledger: {problem.format()}")
    return False


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Guard the accepted-regression ledger: additions are never allowed, "
            "removals are always allowed, and the ledger must stay canonical "
            "(no duplicate / non-normalized / malformed entries)."
        )
    )
    parser.add_argument(
        "--base-ref",
        help="Base git ref to compare against (e.g. origin/main). "
        "Required unless --integrity-only is set.",
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
        "--integrity-only",
        action="store_true",
        help=(
            "Only validate ledger hygiene at --head-ref; skip the base "
            "comparison. Safe to run on any event (push, merge_group, ...)."
        ),
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

    head_text = _read_ref_text(args.head_ref, args.path)
    if head_text is None:
        print(
            f"error: could not read {args.path} from {args.head_ref!r}",
            file=sys.stderr,
        )
        return 1

    # Ledger hygiene is checked on every invocation: a malformed ledger makes
    # both the visible counter and the growth comparison untrustworthy.
    integrity_ok = _report_integrity(repr(args.head_ref), head_text)

    if args.integrity_only:
        return 0 if integrity_ok else 1

    if not args.base_ref:
        parser.error("--base-ref is required unless --integrity-only is set")

    base_text = _read_ref_text(args.base_ref, args.path)
    if base_text is None:
        msg = (
            f"could not read {args.path} from {args.base_ref!r}; "
            "accepted-regression growth check skipped."
        )
        if args.allow_unavailable_base:
            print(f"warning: {msg}", file=sys.stderr)
            return 0 if integrity_ok else 1
        print(f"error: {msg}", file=sys.stderr)
        return 1

    base_entries = entry_set(base_text)
    head_entries = entry_set(head_text)
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
            print(f"::error::accepted-regression ledger must not grow: {entry} was added.")
        print(
            "File a parity issue and track each regression there instead of "
            "adding entries to conformance-accepted-regressions.txt."
        )
        return 1

    if not integrity_ok:
        return 1

    print("Accepted-regression growth gate passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
