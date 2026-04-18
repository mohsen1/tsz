#!/usr/bin/env python3
"""Pick a random conformance failure to work on.

Reads scripts/conformance/conformance-detail.json and selects one failing test
uniformly at random (optionally filtered by category or error code). Prints the
test path and its expected / actual / missing / extra code sets so an agent can
immediately start investigating.

Usage:
    scripts/session/pick-random-failure.py
    scripts/session/pick-random-failure.py --category fingerprint-only
    scripts/session/pick-random-failure.py --category wrong-code --code TS2322
    scripts/session/pick-random-failure.py --seed 42
"""
from __future__ import annotations

import argparse
import json
import random
import sys
from pathlib import Path

DETAIL_FILE = Path(__file__).resolve().parent.parent / "conformance" / "conformance-detail.json"


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


def main() -> int:
    parser = argparse.ArgumentParser(description="Pick a random conformance failure.")
    parser.add_argument("--category", default="any",
                        choices=["any", "fingerprint-only", "wrong-code",
                                 "only-missing", "only-extra",
                                 "all-missing", "false-positive"],
                        help="Restrict to one failure category.")
    parser.add_argument("--code", help="Only pick tests that involve this error code "
                                       "(expected, actual, missing, or extra).")
    parser.add_argument("--seed", type=int, help="Random seed for reproducibility.")
    parser.add_argument("--count", type=int, default=1,
                        help="Number of random failures to print (default 1).")
    parser.add_argument("--paths-only", action="store_true",
                        help="Print just test paths (one per line).")
    args = parser.parse_args()

    if not DETAIL_FILE.exists():
        print(f"error: {DETAIL_FILE} not found. Run conformance snapshot first.",
              file=sys.stderr)
        return 2

    with DETAIL_FILE.open() as f:
        detail = json.load(f)

    failures = detail.get("failures", {})
    candidates: list[tuple[str, dict, str]] = []
    for path, entry in failures.items():
        if not entry:
            continue
        category = classify(entry)
        if args.category != "any" and category != args.category:
            continue
        if args.code:
            codes = (set(entry.get("e", [])) | set(entry.get("a", []))
                     | set(entry.get("m", [])) | set(entry.get("x", [])))
            if args.code not in codes:
                continue
        candidates.append((path, entry, category))

    if not candidates:
        print("No failures match the given filters.", file=sys.stderr)
        return 1

    rng = random.Random(args.seed)
    chosen = rng.sample(candidates, k=min(args.count, len(candidates)))

    for path, entry, category in chosen:
        if args.paths_only:
            print(path)
            continue
        print(f"path:     {path}")
        print(f"category: {category}")
        print(f"expected: {entry.get('e', [])}")
        print(f"actual:   {entry.get('a', [])}")
        if entry.get("m"):
            print(f"missing:  {entry['m']}")
        if entry.get("x"):
            print(f"extra:    {entry['x']}")
        print(f"total candidates: {len(candidates)}")
        print()

    return 0


if __name__ == "__main__":
    sys.exit(main())
