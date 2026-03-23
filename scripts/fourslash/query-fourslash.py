#!/usr/bin/env python3
"""Query fourslash test results offline without re-running tests.

Reads from scripts/fourslash/fourslash-detail.json (produced by the runner with --json-out).

Usage:
  # Show overview
  python3 scripts/fourslash/query-fourslash.py

  # Top failure messages
  python3 scripts/fourslash/query-fourslash.py --top-errors

  # Top feature buckets
  python3 scripts/fourslash/query-fourslash.py --buckets

  # Filter by bucket
  python3 scripts/fourslash/query-fourslash.py --bucket completion

  # Filter by substring in test name
  python3 scripts/fourslash/query-fourslash.py --filter quickInfo

  # Show only failures
  python3 scripts/fourslash/query-fourslash.py --failures

  # Show timeouts
  python3 scripts/fourslash/query-fourslash.py --timeouts

  # Export paths for piping
  python3 scripts/fourslash/query-fourslash.py --failures --paths-only
"""

import sys
import json
import argparse
from collections import Counter
from pathlib import Path

DETAIL_FILE = Path(__file__).parent / "fourslash-detail.json"


def load_detail():
    if not DETAIL_FILE.exists():
        print(f"Error: {DETAIL_FILE} not found.")
        print("Run fourslash tests with --json-out to generate it.")
        sys.exit(1)
    with open(DETAIL_FILE) as f:
        return json.load(f)


def show_overview(data):
    s = data["summary"]
    print(f"Fourslash Test Results")
    print(f"  Total: {s['total']}")
    print(f"  Passed: {s['passed']} ({s['passRate']}%)")
    print(f"  Failed: {s['failed']}")
    if s.get("timedOut", 0) > 0:
        print(f"  Timed out: {s['timedOut']}")
    print()

    results = data["results"]
    fails = [r for r in results if r["status"] == "fail"]

    # Bucket breakdown
    bucket_pass = Counter()
    bucket_fail = Counter()
    for r in results:
        b = r.get("bucket", "other")
        if r["status"] == "pass":
            bucket_pass[b] += 1
        else:
            bucket_fail[b] += 1

    all_buckets = sorted(set(list(bucket_pass.keys()) + list(bucket_fail.keys())))
    print("Feature buckets:")
    for b in all_buckets:
        p = bucket_pass.get(b, 0)
        f = bucket_fail.get(b, 0)
        total = p + f
        rate = p / total * 100 if total > 0 else 0
        print(f"  {b:>20s}: {p:>5d}/{total:>5d} ({rate:5.1f}%)")
    print()

    # Top error messages
    print("Top failure messages:")
    error_counter = Counter()
    for r in fails:
        msg = r.get("firstFailure", "unknown")
        error_counter[msg[:80]] += 1
    for msg, count in error_counter.most_common(10):
        print(f"  {count:>4d}  {msg}")


def show_buckets(data):
    results = data["results"]
    bucket_pass = Counter()
    bucket_fail = Counter()
    bucket_timeout = Counter()
    for r in results:
        b = r.get("bucket", "other")
        if r["status"] == "pass":
            bucket_pass[b] += 1
        elif r["status"] == "timeout":
            bucket_timeout[b] += 1
        else:
            bucket_fail[b] += 1

    all_buckets = sorted(set(list(bucket_pass.keys()) + list(bucket_fail.keys()) + list(bucket_timeout.keys())))
    print(f"{'Bucket':>20s}  {'Pass':>6s}  {'Fail':>6s}  {'Timeout':>7s}  {'Total':>6s}  {'Rate':>6s}")
    print("-" * 65)
    for b in all_buckets:
        p = bucket_pass.get(b, 0)
        f = bucket_fail.get(b, 0)
        t = bucket_timeout.get(b, 0)
        total = p + f + t
        rate = p / total * 100 if total > 0 else 0
        print(f"  {b:>20s}  {p:>5d}  {f:>5d}  {t:>7d}  {total:>5d}  {rate:5.1f}%")


def show_top_errors(data, top=20):
    results = data["results"]
    fails = [r for r in results if r["status"] in ("fail", "timeout")]

    print(f"Top failure messages ({len(fails)} failures):")
    error_counter = Counter()
    for r in fails:
        msg = r.get("firstFailure", "unknown")
        error_counter[msg[:100]] += 1
    for msg, count in error_counter.most_common(top):
        print(f"  {count:>4d}  {msg}")


def show_failures(data, top=40, paths_only=False):
    results = data["results"]
    fails = [r for r in results if r["status"] in ("fail", "timeout")]
    fails.sort(key=lambda r: r["file"])

    if paths_only:
        for r in fails:
            print(r["file"])
        return

    print(f"Failures: {len(fails)}")
    for r in fails[:top]:
        status = "TIMEOUT" if r["timedOut"] else "FAIL"
        bucket = r.get("bucket", "")
        err = r.get("firstFailure", "")[:60]
        print(f"  [{status:>7s}] {r['name']:40s}  [{bucket}]  {err}")
    if len(fails) > top:
        print(f"  ... and {len(fails) - top} more")


def show_bucket(data, bucket, top=40, paths_only=False):
    results = data["results"]
    matches = [r for r in results if r.get("bucket") == bucket]

    if paths_only:
        for r in matches:
            if r["status"] != "pass":
                print(r["file"])
        return

    passing = sum(1 for r in matches if r["status"] == "pass")
    failing = sum(1 for r in matches if r["status"] != "pass")
    total = len(matches)
    rate = passing / total * 100 if total > 0 else 0
    print(f"Bucket '{bucket}': {passing}/{total} ({rate:.1f}%)")
    print()

    fails = [r for r in matches if r["status"] != "pass"]
    if fails:
        print(f"Failures ({len(fails)}):")
        for r in fails[:top]:
            err = r.get("firstFailure", "")[:60]
            print(f"  {r['name']:40s}  {err}")
        if len(fails) > top:
            print(f"  ... and {len(fails) - top} more")


def show_timeouts(data, top=40):
    results = data["results"]
    timeouts = [r for r in results if r.get("timedOut")]
    print(f"Timeouts: {len(timeouts)}")
    for r in timeouts[:top]:
        bucket = r.get("bucket", "")
        print(f"  {r['name']:40s}  [{bucket}]")
    if len(timeouts) > top:
        print(f"  ... and {len(timeouts) - top} more")


def show_filter(data, pattern, top=40, paths_only=False):
    results = data["results"]
    lower = pattern.lower()
    matches = [r for r in results if lower in r["name"].lower() or lower in r["file"].lower()]

    if paths_only:
        for r in matches:
            print(r["file"])
        return

    passing = sum(1 for r in matches if r["status"] == "pass")
    print(f"Tests matching '{pattern}': {len(matches)} ({passing} passing)")
    for r in matches[:top]:
        bucket = r.get("bucket", "")
        err = r.get("firstFailure", "")[:50] if r["status"] != "pass" else ""
        print(f"  [{r['status']:>7s}] {r['name']:40s}  [{bucket}]  {err}")
    if len(matches) > top:
        print(f"  ... and {len(matches) - top} more")


def main():
    parser = argparse.ArgumentParser(description="Query fourslash test results offline")
    parser.add_argument("--buckets", action="store_true", help="Show feature bucket breakdown")
    parser.add_argument("--bucket", type=str, help="Filter by feature bucket")
    parser.add_argument("--top-errors", action="store_true", help="Show top error messages")
    parser.add_argument("--failures", action="store_true", help="Show all failures")
    parser.add_argument("--timeouts", action="store_true", help="Show timeouts")
    parser.add_argument("--filter", type=str, help="Filter by substring")
    parser.add_argument("--paths-only", action="store_true", help="Output only file paths")
    parser.add_argument("--top", type=int, default=40, help="Limit rows shown")
    args = parser.parse_args()

    data = load_detail()

    if args.buckets:
        show_buckets(data)
    elif args.bucket:
        show_bucket(data, args.bucket, args.top, args.paths_only)
    elif args.top_errors:
        show_top_errors(data, args.top)
    elif args.failures:
        show_failures(data, args.top, args.paths_only)
    elif args.timeouts:
        show_timeouts(data, args.top)
    elif args.filter:
        show_filter(data, args.filter, args.top, args.paths_only)
    else:
        show_overview(data)


if __name__ == "__main__":
    main()
