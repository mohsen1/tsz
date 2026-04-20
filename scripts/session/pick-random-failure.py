#!/usr/bin/env python3
"""Pick random conformance failures to work on.

Reads scripts/conformance/conformance-detail.json and selects one or more
failing tests. Supports both uniform selection and the campaign tier weighting
used by the session protocol.
"""

from __future__ import annotations

import argparse
import json
import random
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
DETAIL_PATH = REPO_ROOT / "scripts" / "conformance" / "conformance-detail.json"

TIER_OF = {
    "fingerprint-only": 1,
    "wrong-code": 2,
    "only-missing": 2,
    "only-extra": 2,
    "all-missing": 3,
    "false-positive": 3,
}

TIER_WEIGHT = {1: 0.50, 2: 0.30, 3: 0.20}


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
    category = classify(entry)
    missing = entry.get("m", [])
    extra = entry.get("x", [])

    if args.category != "any" and category != args.category:
        return False
    if args.tier is not None and TIER_OF[category] != args.tier:
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


def build_payload(path: str, entry: dict, category: str, pool_size: int) -> dict:
    return {
        "path": path,
        "category": category,
        "tier": TIER_OF[category],
        "expected": entry.get("e", []),
        "actual": entry.get("a", []),
        "missing": entry.get("m", []),
        "extra": entry.get("x", []),
        "diff": diff(entry),
        "pool_size": pool_size,
    }


def print_payload(payload: dict, *, json_output: bool, paths_only: bool) -> None:
    if paths_only:
        print(payload["path"])
        return

    if json_output:
        print(json.dumps(payload))
        return

    expected = ",".join(payload["expected"]) or "-"
    actual = ",".join(payload["actual"]) or "-"
    missing = ",".join(payload["missing"]) or "-"
    extra = ",".join(payload["extra"]) or "-"

    print(f"path:     {payload['path']}")
    print(f"category: {payload['category']}")
    print(f"tier:     {payload['tier']}")
    print(f"expected: {expected}")
    print(f"actual:   {actual}")
    print(f"missing:  {missing}")
    print(f"extra:    {extra}")
    print(f"diff:     {payload['diff']}")
    print(f"pool:     {payload['pool_size']}")
    print()


def choose_picks(
    candidates: list[tuple[str, dict, str]], args: argparse.Namespace, rng: random.Random
) -> list[dict]:
    count = min(args.count, len(candidates))

    if not args.weighted_tier:
        picks = rng.sample(candidates, count)
        return [
            build_payload(path, entry, category, len(candidates))
            for path, entry, category in picks
        ]

    buckets: dict[int, list[tuple[str, dict, str]]] = {1: [], 2: [], 3: []}
    for candidate in candidates:
        buckets[TIER_OF[candidate[2]]].append(candidate)

    picks = []
    for _ in range(count):
        tiers = [tier for tier, bucket in buckets.items() if bucket]
        weights = [TIER_WEIGHT[tier] for tier in tiers]
        tier = rng.choices(tiers, weights=weights, k=1)[0]
        bucket = buckets[tier]
        index = rng.randrange(len(bucket))
        path, entry, category = bucket.pop(index)
        picks.append(build_payload(path, entry, category, len(bucket) + 1))

    return picks


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
    parser.add_argument(
        "--tier",
        type=int,
        choices=[1, 2, 3],
        help="restrict to one campaign tier",
    )
    parser.add_argument(
        "--weighted-tier",
        action="store_true",
        help="pick using campaign tier weights instead of uniform sampling",
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
    parser.add_argument(
        "--json",
        action="store_true",
        help="print each pick as one JSON object per line",
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
    picks = choose_picks(candidates, args, rng)

    for payload in picks:
        print_payload(payload, json_output=args.json, paths_only=args.paths_only)

    print(f"{len(candidates)} candidates matched; picked {len(picks)}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
