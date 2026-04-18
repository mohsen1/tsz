#!/usr/bin/env python3
"""Pick a random conformance failure to work on.

Reads scripts/conformance/conformance-detail.json and prints a random
failing test along with its expected/actual/missing/extra codes.

Usage:
    scripts/session/pick-random-failure.py                 # any failure
    scripts/session/pick-random-failure.py --close 2       # diff <= N
    scripts/session/pick-random-failure.py --one-extra     # one extra code
    scripts/session/pick-random-failure.py --one-missing   # one missing code
    scripts/session/pick-random-failure.py --seed 42       # reproducible pick
    scripts/session/pick-random-failure.py --count 5       # print N picks
    scripts/session/pick-random-failure.py --paths-only    # test paths only
"""

from __future__ import annotations

import argparse
import json
import random
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
DETAIL_PATH = REPO_ROOT / "scripts" / "conformance" / "conformance-detail.json"


def load_failures() -> dict[str, dict]:
    if not DETAIL_PATH.exists():
        sys.exit(f"error: {DETAIL_PATH} not found; run `scripts/conformance/conformance.sh snapshot` first")
    with DETAIL_PATH.open() as f:
        data = json.load(f)
    return data.get("failures", {})


def diff(entry: dict) -> int:
    return len(entry.get("m", [])) + len(entry.get("x", []))


def matches(entry: dict, args: argparse.Namespace) -> bool:
    missing = entry.get("m", [])
    extra = entry.get("x", [])
    if args.one_missing and not (len(missing) == 1 and len(extra) == 0):
        return False
    if args.one_extra and not (len(extra) == 1 and len(missing) == 0):
        return False
    if args.close is not None and diff(entry) > args.close:
        return False
    if args.code and args.code not in (entry.get("e", []) + entry.get("a", [])):
        return False
    if args.missing_code and args.missing_code not in missing:
        return False
    if args.extra_code and args.extra_code not in extra:
        return False
    return True


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--close", type=int, help="only failures with diff <= N")
    parser.add_argument("--one-missing", action="store_true", help="only 1-missing-0-extra failures")
    parser.add_argument("--one-extra", action="store_true", help="only 0-missing-1-extra failures")
    parser.add_argument("--code", help="only failures involving this code (expected or actual)")
    parser.add_argument("--missing-code", help="only failures where this code is missing")
    parser.add_argument("--extra-code", help="only failures where we emit this code extra")
    parser.add_argument("--seed", type=int, help="random seed for reproducibility")
    parser.add_argument("--count", type=int, default=1, help="number of failures to pick")
    parser.add_argument("--paths-only", action="store_true", help="print test paths only")
    args = parser.parse_args()

    failures = load_failures()
    candidates = [(path, entry) for path, entry in failures.items() if matches(entry, args)]
    if not candidates:
        sys.exit("no failures match the requested filters")

    rng = random.Random(args.seed)
    picks = rng.sample(candidates, min(args.count, len(candidates)))

    for path, entry in picks:
        if args.paths_only:
            print(path)
            continue
        expected = ",".join(entry.get("e", [])) or "-"
        actual = ",".join(entry.get("a", [])) or "-"
        missing = ",".join(entry.get("m", [])) or "-"
        extra = ",".join(entry.get("x", [])) or "-"
        print(f"path:     {path}")
        print(f"expected: {expected}")
        print(f"actual:   {actual}")
        print(f"missing:  {missing}")
        print(f"extra:    {extra}")
        print(f"diff:     {diff(entry)}")
        print()

    print(f"{len(candidates)} candidates matched; picked {len(picks)}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
