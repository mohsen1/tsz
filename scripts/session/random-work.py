#!/usr/bin/env python3
"""Pick a random conformance failure and print a work-ready briefing.

Defaults to "close-to-passing" failures (diff <= 2) because those are the
highest-probability, lowest-effort targets for a single iteration. Pass
``--any`` to sample from all failures.

Reads:
  scripts/conformance/conformance-detail.json   (per-test failure data)
  scripts/conformance/tsc-cache-full.json       (tsc's expected fingerprints)

Output: a single picked failure with its codes, diff, test-case path, and
the tsc expected diagnostics for that test (when available).
"""

from __future__ import annotations

import argparse
import json
import random
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
DETAIL_PATH = REPO_ROOT / "scripts" / "conformance" / "conformance-detail.json"
TSC_CACHE_PATH = REPO_ROOT / "scripts" / "conformance" / "tsc-cache-full.json"


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
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--any", action="store_true", help="sample from all failures (default: diff<=2)")
    ap.add_argument("--diff", type=int, default=2, help="max diff when not --any (default 2)")
    ap.add_argument("--category", help="restrict to one failure category")
    ap.add_argument("--code", help="only failures involving this code")
    ap.add_argument("--seed", type=int, help="reproducible seed")
    args = ap.parse_args()

    if not DETAIL_PATH.exists():
        sys.exit(f"error: {DETAIL_PATH} not found. Run: scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot")

    with DETAIL_PATH.open() as f:
        failures: dict[str, dict] = json.load(f).get("failures", {})

    def diff(e: dict) -> int:
        return len(e.get("m", [])) + len(e.get("x", []))

    candidates = []
    for path, entry in failures.items():
        if not entry:
            continue
        cat = classify(entry)
        if args.category and cat != args.category:
            continue
        if args.code:
            codes = set(entry.get("e", [])) | set(entry.get("a", [])) | set(entry.get("m", [])) | set(entry.get("x", []))
            if args.code not in codes:
                continue
        if not args.any and diff(entry) > args.diff:
            continue
        candidates.append((path, entry, cat))

    if not candidates:
        sys.exit("no failures matched filters")

    rng = random.Random(args.seed)
    path, entry, cat = rng.choice(candidates)

    tsc_fp = []
    if TSC_CACHE_PATH.exists():
        with TSC_CACHE_PATH.open() as f:
            cache = json.load(f)
        record = cache.get(path) or cache.get(Path(path).as_posix()) or {}
        tsc_fp = record.get("diagnostic_fingerprints", [])

    print("=" * 70)
    print(f"PICKED  : {path}")
    print(f"category: {cat}")
    print(f"expected: {','.join(entry.get('e', [])) or '-'}")
    print(f"actual  : {','.join(entry.get('a', [])) or '-'}")
    print(f"missing : {','.join(entry.get('m', [])) or '-'}")
    print(f"extra   : {','.join(entry.get('x', [])) or '-'}")
    print(f"diff    : {diff(entry)}")
    print("=" * 70)

    test_file = REPO_ROOT / path
    if test_file.exists():
        print(f"source  : {test_file}")
    else:
        print(f"source  : {test_file} (NOT FOUND — is the TypeScript submodule initialized?)")

    if tsc_fp:
        print()
        print("tsc expected diagnostics:")
        for fp in tsc_fp:
            print(f"  [{fp.get('code')}] {fp.get('file')}:{fp.get('line')}:{fp.get('column')}  {fp.get('message_key','')[:80]}")

    print()
    print(f"{len(candidates)} candidates matched.")
    print()
    basename = Path(path).stem
    print("Next steps:")
    print(f"  ./scripts/conformance/conformance.sh run --filter '{basename}' --verbose")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
