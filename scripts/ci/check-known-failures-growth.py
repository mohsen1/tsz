#!/usr/bin/env python3
"""Guard the known-failures unit baseline (scripts/ci/known-failures.txt, #15646).

Green unit CI means "no unit failures outside the baseline", so the baseline
itself must only move in reviewed, intentional ways:

* SHRINK (removing entries) is always allowed — a fixed test drops out.
* GROWTH (adding entries) is rejected, with two explicit escapes:
  - bootstrap: the base file is not yet reconciled (no
    ``# baseline-status: reconciled`` marker), so the first reconcile's
    additions are the point of the change;
  - ``TSZ_KNOWN_FAILURES_ALLOW_GROWTH=1`` in the environment — a deliberate
    re-reconcile. CI does not set it, so growth can only land through a
    reviewed change that sets it (or a maintainer running the reconcile
    locally and saying so in the PR).
* Un-reconciling (the base had the marker, the head does not) is rejected —
  dropping the marker would silently flip known-failures-check.mjs back to
  advisory mode and disarm the gate.
* Integrity: entries must be unique, sorted, and shaped like
  ``binary-id::test-name`` so diffs stay reviewable and the checker's set
  semantics hold.

Usage:
    # Growth + integrity (default): additions rejected, removals allowed.
    python3 scripts/ci/check-known-failures-growth.py --base-ref origin/main

    # Fetch origin/main first (shallow CI checkouts), fall back to a warning
    # when the remote is unreachable (sandboxed/offline runs).
    python3 scripts/ci/check-known-failures-growth.py \
        --fetch-base --allow-unavailable-base

    # Integrity only: validate HEAD's baseline hygiene without a base ref.
    python3 scripts/ci/check-known-failures-growth.py --integrity-only
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys

BASELINE_PATH = "scripts/ci/known-failures.txt"
RECONCILED_MARKER = "# baseline-status: reconciled"
ALLOW_GROWTH_ENV = "TSZ_KNOWN_FAILURES_ALLOW_GROWTH"


def parse_entries(text: str) -> list[str]:
    """Non-comment, non-blank lines in file order (duplicates preserved)."""
    entries = []
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        entries.append(line)
    return entries


def is_reconciled(text: str) -> bool:
    return any(line.strip() == RECONCILED_MARKER for line in text.splitlines())


def check_integrity(text: str) -> list[str]:
    """Hygiene problems in a baseline text (empty list when clean)."""
    entries = parse_entries(text)
    problems = []
    seen = set()
    for entry in entries:
        if entry in seen:
            problems.append(f"duplicate entry: {entry}")
        seen.add(entry)
        # nextest ids are `binary-id::test-name`; both halves are Rust paths,
        # so whitespace inside an entry means a mangled line.
        if "::" not in entry:
            problems.append(f"malformed entry (expected binary-id::test-name): {entry}")
        elif any(ch.isspace() for ch in entry):
            problems.append(f"malformed entry (embedded whitespace): {entry}")
    if entries != sorted(entries):
        problems.append("entries are not sorted (regenerate with --update)")
    return problems


def check_growth(
    base_text: str, head_text: str, allow_growth: bool
) -> tuple[list[str], list[str], list[str]]:
    """Return (problems, added, removed) for a base -> head baseline move."""
    base_entries = set(parse_entries(base_text))
    head_entries = set(parse_entries(head_text))
    added = sorted(head_entries - base_entries)
    removed = sorted(base_entries - head_entries)
    problems = []
    if is_reconciled(base_text) and not is_reconciled(head_text):
        problems.append(
            "the reconciled marker was removed; that would flip "
            "known-failures-check.mjs back to advisory mode. Keep the marker "
            "(shrink by deleting entries, not the marker)."
        )
    if added and is_reconciled(base_text) and not allow_growth:
        problems.append(
            f"{len(added)} entr{'y was' if len(added) == 1 else 'ies were'} "
            "added to the baseline. The baseline may only shrink in normal "
            "PRs: fix the regression instead. A deliberate re-reconcile must "
            f"run with {ALLOW_GROWTH_ENV}=1 and say so in the PR body."
        )
    return problems, added, removed


def _read_ref_text(ref: str, path: str) -> str | None:
    try:
        return subprocess.check_output(
            ["git", "show", f"{ref}:{path}"],
            text=True,
            stderr=subprocess.PIPE,
        )
    except subprocess.CalledProcessError:
        return None


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Guard the known-failures baseline: additions need an explicit, "
            "reviewed escape; removals are always allowed; the reconciled "
            "marker must never be dropped; the file must stay canonical."
        )
    )
    parser.add_argument(
        "--base-ref",
        default="origin/main",
        help="Base git ref to compare against (default: origin/main).",
    )
    parser.add_argument(
        "--head-ref",
        default="HEAD",
        help="Head git ref to inspect (default: HEAD).",
    )
    parser.add_argument(
        "--path",
        default=BASELINE_PATH,
        help=f"Path to the baseline file (default: {BASELINE_PATH}).",
    )
    parser.add_argument(
        "--integrity-only",
        action="store_true",
        help="Only validate baseline hygiene at --head-ref; skip the base comparison.",
    )
    parser.add_argument(
        "--fetch-base",
        action="store_true",
        help=(
            "Run `git fetch --no-tags --depth=1 origin main` first and compare "
            "against FETCH_HEAD (for shallow CI checkouts)."
        ),
    )
    parser.add_argument(
        "--allow-unavailable-base",
        action="store_true",
        help=(
            "Warn and skip the growth comparison when the base ref cannot be "
            "read (offline/sandboxed runs). Integrity is still enforced."
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

    integrity_problems = check_integrity(head_text)
    for problem in integrity_problems:
        print(f"::error::known-failures baseline: {problem}")
    if not integrity_problems:
        print("known-failures baseline integrity passed.")

    if args.integrity_only:
        return 1 if integrity_problems else 0

    base_ref = args.base_ref
    if args.fetch_base:
        fetch = subprocess.run(
            ["git", "fetch", "--no-tags", "--depth=1", "origin", "main"],
            capture_output=True,
            text=True,
        )
        if fetch.returncode == 0:
            base_ref = "FETCH_HEAD"
        else:
            print(
                f"warning: could not fetch origin main: {fetch.stderr.strip()}",
                file=sys.stderr,
            )

    base_text = _read_ref_text(base_ref, args.path)
    if base_text is None:
        msg = (
            f"could not read {args.path} from {base_ref!r}; "
            "known-failures growth check skipped."
        )
        if args.allow_unavailable_base:
            print(f"warning: {msg}", file=sys.stderr)
            return 1 if integrity_problems else 0
        print(f"error: {msg}", file=sys.stderr)
        return 1

    allow_growth = os.environ.get(ALLOW_GROWTH_ENV, "") == "1"
    growth_problems, added, removed = check_growth(base_text, head_text, allow_growth)

    print(
        f"known-failures baseline: {len(parse_entries(base_text))} entr(ies) at "
        f"{base_ref!r}, {len(parse_entries(head_text))} at {args.head_ref!r}."
    )
    if removed:
        print(f"Removed entries ({len(removed)}) — shrink ratchet:")
        for entry in removed:
            print(f"  - {entry}")
    if added:
        print(f"Added entries ({len(added)}):")
        for entry in added:
            print(f"  + {entry}")
        if not is_reconciled(base_text):
            print("Bootstrap reconcile (base baseline was unreconciled): growth allowed.")
        elif allow_growth:
            print(f"{ALLOW_GROWTH_ENV}=1 set: growth allowed (deliberate re-reconcile).")
    for problem in growth_problems:
        print(f"::error::known-failures baseline: {problem}")

    if growth_problems or integrity_problems:
        return 1
    print("known-failures growth gate passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
