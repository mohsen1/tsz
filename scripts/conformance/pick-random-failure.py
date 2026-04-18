#!/usr/bin/env python3
"""Pick a random conformance failure to work on.

Reads scripts/conformance/conformance-detail.json (offline, no test runs).

Usage:
  pick-random-failure.py                  # any failure
  pick-random-failure.py --code TS2322    # only failures involving TS2322
  pick-random-failure.py --category fp    # only fingerprint-only failures
  pick-random-failure.py --category wrong # only wrong-code failures
  pick-random-failure.py --close 2        # only failures with diff <= 2
  pick-random-failure.py --seed 42        # deterministic pick
  pick-random-failure.py --count 5        # pick N failures
"""
import argparse
import json
import os
import random
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
DETAIL_PATH = REPO_ROOT / "scripts" / "conformance" / "conformance-detail.json"


def categorize(rec: dict) -> str:
    expected = set(rec.get("e", []))
    actual = set(rec.get("a", []))
    missing = set(rec.get("m", []))
    extra = set(rec.get("x", []))

    if not expected and actual:
        return "false-positive"
    if expected and not actual:
        return "all-missing"
    if expected == actual and not missing and not extra:
        return "fingerprint-only"
    if missing and extra:
        return "wrong-codes"
    if missing and not extra:
        return "missing-only"
    if extra and not missing:
        return "extra-only"
    return "other"


def diff_size(rec: dict) -> int:
    return len(rec.get("m", [])) + len(rec.get("x", []))


def matches(rec: dict, args) -> bool:
    expected = set(rec.get("e", []))
    actual = set(rec.get("a", []))

    if args.code:
        if args.code not in expected and args.code not in actual:
            return False

    if args.category:
        cat = categorize(rec)
        wanted = args.category.lower()
        aliases = {
            "fp": "fingerprint-only",
            "fingerprint": "fingerprint-only",
            "wrong": "wrong-codes",
            "false-positive": "false-positive",
            "fp-positive": "false-positive",
            "missing": "all-missing",
            "all-missing": "all-missing",
            "extra-only": "extra-only",
            "missing-only": "missing-only",
        }
        wanted = aliases.get(wanted, wanted)
        if cat != wanted:
            return False

    if args.close is not None:
        if diff_size(rec) > args.close:
            return False

    return True


def main() -> int:
    parser = argparse.ArgumentParser(description="Pick a random conformance failure to work on.")
    parser.add_argument("--code", help="Filter by error code (e.g. TS2322)")
    parser.add_argument(
        "--category",
        help="Filter by category: fp, wrong, false-positive, all-missing, missing-only, extra-only",
    )
    parser.add_argument("--close", type=int, help="Filter to diff <= N")
    parser.add_argument("--seed", type=int, help="Deterministic random seed")
    parser.add_argument("--count", type=int, default=1, help="How many failures to pick")
    parser.add_argument("--paths-only", action="store_true", help="Print only test paths")
    args = parser.parse_args()

    if not DETAIL_PATH.exists():
        print(f"error: {DETAIL_PATH} not found", file=sys.stderr)
        return 2

    with DETAIL_PATH.open() as f:
        detail = json.load(f)

    failures = [(path, rec) for path, rec in detail["failures"].items() if rec]
    failures = [(p, r) for p, r in failures if matches(r, args)]

    if not failures:
        print("no failures matched filters", file=sys.stderr)
        return 1

    if args.seed is None:
        seed = int.from_bytes(os.urandom(8), "big")
    else:
        seed = args.seed
    rng = random.Random(seed)

    count = min(args.count, len(failures))
    picks = rng.sample(failures, count)

    if args.paths_only:
        for path, _ in picks:
            print(path)
        return 0

    print(f"# seed={seed}  pool={len(failures)}  picked={count}")
    for path, rec in picks:
        cat = categorize(rec)
        e = ",".join(sorted(rec.get("e", []))) or "-"
        a = ",".join(sorted(rec.get("a", []))) or "-"
        m = ",".join(sorted(rec.get("m", []))) or "-"
        x = ",".join(sorted(rec.get("x", []))) or "-"
        print(f"{path}")
        print(f"  category : {cat}")
        print(f"  expected : {e}")
        print(f"  actual   : {a}")
        print(f"  missing  : {m}")
        print(f"  extra    : {x}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
