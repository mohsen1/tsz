#!/usr/bin/env python3
"""Query conformance snapshot data without re-running tests.

Reads from scripts/conformance-detail.json (produced by `conformance.sh snapshot`).

Usage:
  # Show overview of what to work on next
  python3 scripts/query-conformance.py

  # Show tests fixable by adding a single missing code (highest impact)
  python3 scripts/query-conformance.py --one-missing

  # Show false positive breakdown
  python3 scripts/query-conformance.py --false-positives

  # Show tests that need a specific code
  python3 scripts/query-conformance.py --code TS2454

  # Show tests fixable by removing a single extra code
  python3 scripts/query-conformance.py --one-extra

  # List all tests failing with a specific extra code (false emissions)
  python3 scripts/query-conformance.py --extra-code TS7053

  # Show tests closest to passing (diff <= N)
  python3 scripts/query-conformance.py --close 2

  # Export test paths for a code to feed into conformance runner
  python3 scripts/query-conformance.py --code TS2454 --paths-only
"""

import sys
import json
import argparse
from collections import Counter
from pathlib import Path

DETAIL_FILE = Path(__file__).parent / "conformance-detail.json"


def load_detail():
    if not DETAIL_FILE.exists():
        print(f"Error: {DETAIL_FILE} not found.")
        print("Run: ./scripts/conformance.sh snapshot")
        sys.exit(1)
    with open(DETAIL_FILE) as f:
        return json.load(f)


def show_overview(data):
    s = data["summary"]
    a = data["aggregates"]
    print(f"Conformance: {s['passed']}/{s['total']} ({s['passed']/s['total']*100:.1f}%)")
    print()

    cats = a["categories"]
    print("Failure categories:")
    print(f"  False positives (expected 0, we emit):  {cats['false_positive']}")
    print(f"  All missing (expected errors, we emit 0): {cats['all_missing']}")
    print(f"  Wrong codes (both have, codes differ):  {cats['wrong_code']}")
    print(f"  Close to passing (diff <= 2):           {cats['close_to_passing']}")
    print()

    print("Quick wins — add 1 missing code, 0 extra (instant pass):")
    for item in a["one_missing_zero_extra"][:15]:
        print(f"  {item['code']:>8s}: {item['count']:>3d} tests")
    print()

    print("Quick wins — remove 1 extra code, 0 missing (instant pass):")
    for item in a["one_extra_zero_missing"][:15]:
        print(f"  {item['code']:>8s}: {item['count']:>3d} tests")
    print()

    print("Not implemented codes (never emitted by tsz):")
    for item in a["not_implemented_codes"][:15]:
        print(f"  {item['code']:>8s}: {item['count']:>3d} tests need it")
    print()

    print("Partially implemented (emitted sometimes, missing others):")
    for item in a["partial_codes"][:15]:
        print(f"  {item['code']:>8s}: missing in {item['count']:>3d} tests")


def show_one_missing(data):
    a = data["aggregates"]
    items = a["one_missing_zero_extra"]
    if not items:
        print("No tests are exactly 1 missing code away from passing.")
        return
    total = sum(i["count"] for i in items)
    print(f"Tests fixable by adding 1 missing code (0 extra): {total} total")
    print()
    for item in items:
        print(f"  {item['code']:>8s}: {item['count']:>3d} tests would pass")


def show_one_extra(data):
    a = data["aggregates"]
    items = a["one_extra_zero_missing"]
    if not items:
        print("No tests are exactly 1 extra code away from passing.")
        return
    total = sum(i["count"] for i in items)
    print(f"Tests fixable by removing 1 extra code (0 missing): {total} total")
    print()
    for item in items:
        print(f"  {item['code']:>8s}: {item['count']:>3d} tests would pass")


def show_false_positives(data):
    a = data["aggregates"]
    failures = data["failures"]
    print(f"False positives: {a['categories']['false_positive']} tests")
    print()
    print("Top codes emitted incorrectly:")
    for item in a["false_positive_codes"][:20]:
        print(f"  {item['code']:>8s}: {item['count']:>3d} tests")
    print()

    # List actual false positive tests grouped by code
    fp_tests = {}
    for path, f in failures.items():
        if not f.get("e") and f.get("a"):
            for code in set(f["a"]):
                fp_tests.setdefault(code, []).append(path)

    for item in a["false_positive_codes"][:5]:
        code = item["code"]
        tests = fp_tests.get(code, [])
        print(f"\n{code} false positives ({len(tests)} tests):")
        for t in sorted(tests)[:10]:
            basename = t.rsplit("/", 1)[-1] if "/" in t else t
            print(f"  {basename}")
        if len(tests) > 10:
            print(f"  ... and {len(tests) - 10} more")


def show_code(data, code, paths_only=False):
    failures = data["failures"]
    missing_tests = []
    extra_tests = []
    for path, f in sorted(failures.items()):
        if code in f.get("m", []):
            missing_tests.append((path, f))
        if code in f.get("x", []):
            extra_tests.append((path, f))

    if paths_only:
        for path, _ in missing_tests + extra_tests:
            print(path)
        return

    print(f"Code {code}:")
    print(f"  Missing in {len(missing_tests)} tests (need to add)")
    print(f"  Extra in {len(extra_tests)} tests (need to remove)")
    print()

    if missing_tests:
        # Sub-categorize missing tests
        only_this = [(p, f) for p, f in missing_tests if f.get("m") == [code] and not f.get("x")]
        print(f"  Would-pass-if-added (only missing code, 0 extra): {len(only_this)}")
        for p, f in only_this[:20]:
            basename = p.rsplit("/", 1)[-1] if "/" in p else p
            exp = ",".join(f.get("e", []))
            print(f"    {basename}  expected=[{exp}]")
        if len(only_this) > 20:
            print(f"    ... and {len(only_this) - 20} more")
        print()

        also_need = [(p, f) for p, f in missing_tests if f.get("m") != [code] or f.get("x")]
        if also_need:
            print(f"  Also missing {code} but need other fixes too: {len(also_need)}")
            for p, f in also_need[:10]:
                basename = p.rsplit("/", 1)[-1] if "/" in p else p
                m = ",".join(f.get("m", []))
                x = ",".join(f.get("x", []))
                print(f"    {basename}  missing=[{m}]  extra=[{x}]")
            if len(also_need) > 10:
                print(f"    ... and {len(also_need) - 10} more")

    if extra_tests:
        print(f"\n  Extra {code} in {len(extra_tests)} tests:")
        only_this = [(p, f) for p, f in extra_tests if f.get("x") == [code] and not f.get("m")]
        print(f"    Would-pass-if-removed (only extra code, 0 missing): {len(only_this)}")
        for p, f in only_this[:10]:
            basename = p.rsplit("/", 1)[-1] if "/" in p else p
            print(f"      {basename}")


def show_extra_code(data, code):
    failures = data["failures"]
    tests = []
    for path, f in sorted(failures.items()):
        if code in f.get("x", []):
            tests.append((path, f))

    print(f"Tests where {code} is emitted as EXTRA ({len(tests)} tests):")
    for p, f in tests[:30]:
        basename = p.rsplit("/", 1)[-1] if "/" in p else p
        m = ",".join(f.get("m", []))
        x = ",".join(f.get("x", []))
        e = ",".join(f.get("e", []))
        print(f"  {basename}  expected=[{e}]  missing=[{m}]  extra=[{x}]")
    if len(tests) > 30:
        print(f"  ... and {len(tests) - 30} more")


def show_close(data, max_diff):
    failures = data["failures"]
    close = []
    for path, f in failures.items():
        missing = f.get("m", [])
        extra = f.get("x", [])
        diff = len(missing) + len(extra)
        if 0 < diff <= max_diff:
            close.append((diff, path, f))
    close.sort()
    print(f"Tests within diff <= {max_diff} of passing: {len(close)}")
    for diff, path, f in close[:40]:
        basename = path.rsplit("/", 1)[-1] if "/" in path else path
        m = ",".join(f.get("m", []))
        x = ",".join(f.get("x", []))
        print(f"  [diff={diff}] {basename}  missing=[{m}]  extra=[{x}]")
    if len(close) > 40:
        print(f"  ... and {len(close) - 40} more")


def main():
    parser = argparse.ArgumentParser(description="Query conformance snapshot offline")
    parser.add_argument("--one-missing", action="store_true", help="Show 1-missing-0-extra tests")
    parser.add_argument("--one-extra", action="store_true", help="Show 1-extra-0-missing tests")
    parser.add_argument("--false-positives", action="store_true", help="Show false positive breakdown")
    parser.add_argument("--code", type=str, help="Show tests involving a specific error code (e.g., TS2454)")
    parser.add_argument("--extra-code", type=str, help="Show tests where a code is emitted as extra")
    parser.add_argument("--close", type=int, help="Show tests within diff <= N of passing")
    parser.add_argument("--paths-only", action="store_true", help="Output only test paths (for piping)")
    args = parser.parse_args()

    data = load_detail()

    if args.one_missing:
        show_one_missing(data)
    elif args.one_extra:
        show_one_extra(data)
    elif args.false_positives:
        show_false_positives(data)
    elif args.code:
        show_code(data, args.code, args.paths_only)
    elif args.extra_code:
        show_extra_code(data, args.extra_code)
    elif args.close is not None:
        show_close(data, args.close)
    else:
        show_overview(data)


if __name__ == "__main__":
    main()
