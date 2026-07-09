#!/usr/bin/env python3
"""Guard the known-failures unit baseline (scripts/ci/known-failures.txt, #15646).

Green unit CI means "no unit failures outside the baseline", so the baseline
itself must only move in reviewed, intentional ways:

* SHRINK (removing entries) is always allowed — a fixed test drops out.
* GROWTH (adding entries) is rejected unless the same diff bumps the
  reconcile generation carried by the marker line
  (``# baseline-status: reconciled r<N>``; the bare marker reads as r1).
  ``known-failures-check.mjs --update --bump-generation`` writes the bump, so
  a deliberate re-reconcile authorizes its own additions inside the reviewed
  artifact — no out-of-band env vars or workflow edits. The bootstrap
  reconcile (base file unreconciled, generation 0) is growth by definition
  and is allowed.
* Lowering the generation — including dropping the marker, which would
  silently flip known-failures-check.mjs back to advisory mode — is rejected.
* Integrity: entries must be unique, sorted, and shaped like
  ``binary-id::test-name`` so diffs stay reviewable and the checker's set
  semantics hold.

The head baseline is read from the working tree (what CI checked out / what
the developer is about to commit); the base comes from ``git show``.

Usage:
    # Growth + integrity (default): compare the working tree to origin/main.
    python3 scripts/ci/check-known-failures-growth.py --base-ref origin/main

    # Shallow CI checkouts: resolve the base, fetching it only if absent, and
    # fall back to a warning when the remote is unreachable (offline runs).
    python3 scripts/ci/check-known-failures-growth.py \
        --fetch-base --allow-unavailable-base

    # Integrity only: validate the working-tree baseline without a base ref.
    python3 scripts/ci/check-known-failures-growth.py --integrity-only
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys

BASELINE_PATH = "scripts/ci/known-failures.txt"
# Must stay in lockstep with RECONCILED_MARKER / RECONCILED_MARKER_RE in
# scripts/ci/known-failures-check.mjs (pinned by a drift test in
# test_check_known_failures_growth.py).
RECONCILED_MARKER = "# baseline-status: reconciled"
RECONCILED_MARKER_RE = re.compile(r"^# baseline-status: reconciled(?: r(\d+))?$")


def parse_entries(text: str) -> list[str]:
    """Non-comment, non-blank lines in file order (duplicates preserved)."""
    entries = []
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        entries.append(line)
    return entries


def baseline_generation(text: str) -> int:
    """Reconcile generation: 0 unreconciled, 1 bare marker, N for ``rN``."""
    for raw in text.splitlines():
        match = RECONCILED_MARKER_RE.match(raw.strip())
        if match:
            return int(match.group(1)) if match.group(1) else 1
    return 0


def check_integrity(text: str) -> list[str]:
    """Hygiene problems in a baseline text (empty list when clean)."""
    entries = parse_entries(text)
    problems = []
    # A marker-like line the generation parser does not understand must be a
    # hard error, not generation 0: reading a mangled/future marker as
    # "unreconciled" would misclassify the next growth as a bootstrap
    # reconcile and wave it through.
    for raw in text.splitlines():
        line = raw.strip()
        if line.startswith("# baseline-status:") and not RECONCILED_MARKER_RE.match(line):
            problems.append(f"unparseable baseline-status marker: {line}")
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


def check_growth(base_text: str, head_text: str) -> dict:
    """Adjudicate a base -> head baseline move.

    Returns a dict with `problems`/`notes` (lists of strings), the
    `added`/`removed` entry lists, and the already-computed
    `base_count`/`head_count`/`base_gen`/`head_gen` so callers can report
    without re-parsing.
    """
    base_entries = set(parse_entries(base_text))
    head_entries = set(parse_entries(head_text))
    added = sorted(head_entries - base_entries)
    removed = sorted(base_entries - head_entries)
    base_gen = baseline_generation(base_text)
    head_gen = baseline_generation(head_text)
    problems = []
    notes = []
    if head_gen < base_gen:
        problems.append(
            f"the reconcile generation went backwards (r{base_gen} -> r{head_gen}"
            f"{' — marker removed' if head_gen == 0 else ''}); that would flip "
            "known-failures-check.mjs toward advisory mode. Keep the marker and "
            "its generation (shrink by deleting entries, not the marker)."
        )
    if added:
        if base_gen == 0:
            notes.append("bootstrap reconcile (base baseline was unreconciled): growth allowed.")
        elif head_gen > base_gen:
            notes.append(
                f"reconcile generation bumped r{base_gen} -> r{head_gen}: growth allowed "
                "(deliberate re-reconcile)."
            )
        else:
            problems.append(
                f"{len(added)} entr{'y was' if len(added) == 1 else 'ies were'} "
                "added to the baseline without bumping the reconcile generation. "
                "The baseline may only shrink in normal PRs: fix the regression "
                "instead. A deliberate re-reconcile regenerates the file with "
                "`node scripts/ci/known-failures-check.mjs --update "
                "--bump-generation` and says so in the PR body."
            )
    return {
        "problems": problems,
        "notes": notes,
        "added": added,
        "removed": removed,
        "base_count": len(base_entries),
        "head_count": len(head_entries),
        "base_gen": base_gen,
        "head_gen": head_gen,
    }


def _read_ref_text(ref: str, path: str) -> str | None:
    try:
        return subprocess.check_output(
            ["git", "show", f"{ref}:{path}"],
            text=True,
            stderr=subprocess.PIPE,
        )
    except subprocess.CalledProcessError:
        return None


def _ref_exists(ref: str) -> bool:
    return (
        subprocess.run(
            ["git", "rev-parse", "-q", "--verify", f"{ref}^{{commit}}"],
            capture_output=True,
        ).returncode
        == 0
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Guard the known-failures baseline: additions require a reconcile-"
            "generation bump in the same diff, removals are always allowed, "
            "the generation can never go backwards, and the file must stay "
            "canonical (no duplicate / unsorted / malformed entries)."
        )
    )
    parser.add_argument(
        "--base-ref",
        default="origin/main",
        help="Base git ref to compare against (default: origin/main).",
    )
    parser.add_argument(
        "--path",
        default=BASELINE_PATH,
        help=f"Path to the baseline file (default: {BASELINE_PATH}).",
    )
    parser.add_argument(
        "--integrity-only",
        action="store_true",
        help="Only validate working-tree baseline hygiene; skip the base comparison.",
    )
    parser.add_argument(
        "--fetch-base",
        action="store_true",
        help=(
            "When --base-ref does not resolve locally (shallow CI checkouts), "
            "run `git fetch --no-tags --depth=1 origin main` and compare "
            "against FETCH_HEAD."
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

    try:
        with open(args.path, encoding="utf-8") as handle:
            head_text = handle.read()
    except OSError as err:
        print(f"error: could not read {args.path}: {err}", file=sys.stderr)
        return 1

    integrity_problems = check_integrity(head_text)
    for problem in integrity_problems:
        print(f"::error::known-failures baseline: {problem}")
    if not integrity_problems:
        print("known-failures baseline integrity passed.")

    if args.integrity_only:
        return 1 if integrity_problems else 0

    base_ref = args.base_ref
    if args.fetch_base and not _ref_exists(base_ref):
        # Derive the fetch source from --base-ref ("origin/main" -> remote
        # origin, ref main; a bare ref fetches from origin). blob:none keeps
        # the shallow fetch to commits+trees; the later `git show` lazily
        # fetches just the one baseline blob.
        if "/" in args.base_ref:
            remote, _, ref = args.base_ref.partition("/")
        else:
            remote, ref = "origin", args.base_ref
        fetch = subprocess.run(
            ["git", "fetch", "--no-tags", "--depth=1", "--filter=blob:none", remote, ref],
            capture_output=True,
            text=True,
        )
        if fetch.returncode == 0:
            base_ref = "FETCH_HEAD"
        else:
            print(
                f"warning: could not fetch {remote} {ref}: {fetch.stderr.strip()}",
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

    growth = check_growth(base_text, head_text)

    print(
        f"known-failures baseline: {growth['base_count']} entr(ies) "
        f"(r{growth['base_gen']}) at {base_ref!r}, "
        f"{growth['head_count']} (r{growth['head_gen']}) in the working tree."
    )
    if growth["removed"]:
        print(f"Removed entries ({len(growth['removed'])}) — shrink ratchet:")
        for entry in growth["removed"]:
            print(f"  - {entry}")
    if growth["added"]:
        print(f"Added entries ({len(growth['added'])}):")
        for entry in growth["added"]:
            print(f"  + {entry}")
    for note in growth["notes"]:
        print(note)
    for problem in growth["problems"]:
        print(f"::error::known-failures baseline: {problem}")

    if growth["problems"] or integrity_problems:
        return 1
    print("known-failures growth gate passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
