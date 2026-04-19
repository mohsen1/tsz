#!/usr/bin/env python3
"""Pick a random conformance failure to work on.

Reads failure data from scripts/conformance/conformance-detail.json and prints
one randomly selected failing test along with its expected/actual/missing/extra
error codes. Useful when an agent needs a starting point without biasing toward
a particular campaign.

Usage:
  python3 scripts/conformance/pick-random-failure.py
  python3 scripts/conformance/pick-random-failure.py --seed 42
  python3 scripts/conformance/pick-random-failure.py --count 5
  python3 scripts/conformance/pick-random-failure.py --category fingerprint-only
  python3 scripts/conformance/pick-random-failure.py --code TS2322
  python3 scripts/conformance/pick-random-failure.py --path-only
"""

from __future__ import annotations

import argparse
import json
import random
import sys
from pathlib import Path

DETAIL_FILE = Path(__file__).parent / "conformance-detail.json"


def classify(entry: dict) -> str:
    expected = entry.get("e", []) or []
    actual = entry.get("a", []) or []
    missing = entry.get("m", []) or []
    extra = entry.get("x", []) or []

    if not expected and actual:
        return "false-positive"
    if expected and not actual:
        return "all-missing"
    if not missing and not extra and expected:
        return "fingerprint-only"
    if missing and not extra:
        return "only-missing"
    if extra and not missing:
        return "only-extra"
    return "wrong-codes"


def load_failures() -> dict:
    if not DETAIL_FILE.exists():
        sys.exit(
            f"error: {DETAIL_FILE} not found. Run "
            "`scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot` first."
        )
    with DETAIL_FILE.open() as fh:
        data = json.load(fh)
    return data.get("failures", {})


def filter_failures(
    failures: dict,
    *,
    category: str | None,
    code: str | None,
) -> list[tuple[str, dict]]:
    items: list[tuple[str, dict]] = []
    for path, entry in failures.items():
        if not entry:
            # Empty payload means we lack detail (often a crash). Keep it as
            # a catch-all under 'unknown' so a random pick can still surface it.
            if category in (None, "unknown"):
                items.append((path, entry))
            continue

        cat = classify(entry)
        if category and cat != category:
            continue

        if code:
            codes = set(entry.get("e", []) or []) | set(entry.get("a", []) or [])
            if code not in codes:
                continue

        items.append((path, entry))
    return items


def format_entry(path: str, entry: dict) -> str:
    if not entry:
        return f"{path}\n  (no diagnostic detail — likely crash or missing snapshot)"

    lines = [path, f"  category: {classify(entry)}"]

    def fmt(key: str, label: str) -> None:
        vals = entry.get(key) or []
        if vals:
            lines.append(f"  {label:<9}: {', '.join(vals)}")

    fmt("e", "expected")
    fmt("a", "actual")
    fmt("m", "missing")
    fmt("x", "extra")
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--seed", type=int, help="Seed RNG for reproducible picks.")
    parser.add_argument("--count", type=int, default=1, help="Number of failures to pick.")
    parser.add_argument(
        "--category",
        choices=[
            "fingerprint-only",
            "only-missing",
            "only-extra",
            "wrong-codes",
            "false-positive",
            "all-missing",
            "unknown",
        ],
        help="Restrict to one failure category.",
    )
    parser.add_argument(
        "--code", help="Only pick failures involving this error code (e.g. TS2322)."
    )
    parser.add_argument(
        "--path-only", action="store_true", help="Print only the test path(s)."
    )
    args = parser.parse_args()

    if args.count < 1:
        parser.error("--count must be >= 1")

    failures = load_failures()
    pool = filter_failures(failures, category=args.category, code=args.code)
    if not pool:
        sys.exit("error: no failures matched the given filters.")

    rng = random.Random(args.seed)
    picks = rng.sample(pool, k=min(args.count, len(pool)))

    if args.path_only:
        for path, _ in picks:
            print(path)
        return 0

    print(f"# Picked {len(picks)} of {len(pool)} candidate failures")
    if args.category:
        print(f"# Filter: category={args.category}")
    if args.code:
        print(f"# Filter: code={args.code}")
    if args.seed is not None:
        print(f"# Seed: {args.seed}")
    print()
    for path, entry in picks:
        print(format_entry(path, entry))
        print()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
