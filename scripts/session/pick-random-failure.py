#!/usr/bin/env python3
"""Pick a random conformance failure to work on.

Reads scripts/conformance/conformance-detail.json and selects one or more
failing tests uniformly at random. Supports both coarse category filters
(`wrong-code`, `fingerprint-only`, etc.) and narrow diff filters
(`--one-missing`, `--extra-code`, `--close`).
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
        sys.exit(
            f"error: {DETAIL_PATH} not found; run "
            "`scripts/conformance/conformance.sh snapshot` first"
        )
    with DETAIL_PATH.open() as f:
        data = json.load(f)
    return data.get("failures", {})


def classify(entry: dict) -> str:
    expected = set(entry.get("e", []))
    actual = set(entry.get("a", []))
    missing = set(entry.get("m", []))
    extra = set(entry.get("x", []))

    if not expected and actual:
        return "false-positive"
    if expected and not actual:
        return "all-missing"
    if expected == actual:
        return "fingerprint-only"
    if missing and not extra:
        return "only-missing"
    if extra and not missing:
        return "only-extra"
    return "wrong-code"


def diff(entry: dict) -> int:
    return len(entry.get("m", [])) + len(entry.get("x", []))


def matches(entry: dict, args: argparse.Namespace) -> bool:
    missing = entry.get("m", [])
    extra = entry.get("x", [])

    if args.category != "any" and classify(entry) != args.category:
        return False
    if args.one_missing and not (len(missing) == 1 and len(extra) == 0):
        return False
    if args.one_extra and not (len(extra) == 1 and len(missing) == 0):
        return False
    if args.close is not None and diff(entry) > args.close:
        return False
    if args.code:
        all_codes = (
            set(entry.get("e", []))
            | set(entry.get("a", []))
            | set(missing)
            | set(extra)
        )
        if args.code not in all_codes:
            return False
    if args.missing_code and args.missing_code not in missing:
        return False
    if args.extra_code and args.extra_code not in extra:
        return False
    return True


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--category",
        default="any",
        choices=[
            "any",
            "fingerprint-only",
            "wrong-code",
            "only-missing",
            "only-extra",
            "all-missing",
            "false-positive",
        ],
        help="restrict to one failure category",
    )
    parser.add_argument("--close", type=int, help="only failures with diff <= N")
    parser.add_argument(
        "--one-missing",
        action="store_true",
        help="only 1-missing-0-extra failures",
    )
    parser.add_argument(
        "--one-extra",
        action="store_true",
        help="only 0-missing-1-extra failures",
    )
    parser.add_argument(
        "--code",
        help="only failures involving this code (expected, actual, missing, or extra)",
    )
    parser.add_argument(
        "--missing-code",
        help="only failures where this code is missing",
    )
    parser.add_argument("--extra-code", help="only failures where this code is extra")
    parser.add_argument("--seed", type=int, help="random seed for reproducibility")
    parser.add_argument(
        "--count",
        type=int,
        default=1,
        help="number of failures to print",
    )
    parser.add_argument(
        "--paths-only",
        action="store_true",
        help="print test paths only",
    )
    args = parser.parse_args()

    failures = load_failures()
    candidates = [
        (path, entry, classify(entry))
        for path, entry in failures.items()
        if entry and matches(entry, args)
    ]
    if not candidates:
        sys.exit("no failures match the requested filters")

    rng = random.Random(args.seed)
    picks = rng.sample(candidates, min(args.count, len(candidates)))

    for path, entry, category in picks:
        if args.paths_only:
            print(path)
            continue

        expected = ",".join(entry.get("e", [])) or "-"
        actual = ",".join(entry.get("a", [])) or "-"
        missing = ",".join(entry.get("m", [])) or "-"
        extra = ",".join(entry.get("x", [])) or "-"

        print(f"path:     {path}")
        print(f"category: {category}")
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
