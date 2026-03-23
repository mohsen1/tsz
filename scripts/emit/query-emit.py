#!/usr/bin/env python3
"""Query emit test results offline without re-running tests.

Reads from scripts/emit/emit-detail.json (produced by the emit runner with --json-out).

Usage:
  # Show overview
  python3 scripts/emit/query-emit.py

  # Top failure messages
  python3 scripts/emit/query-emit.py --top-errors

  # Filter by substring in test name
  python3 scripts/emit/query-emit.py --filter class

  # Show only JS failures or DTS failures
  python3 scripts/emit/query-emit.py --js-failures
  python3 scripts/emit/query-emit.py --dts-failures

  # Tests closest to passing (e.g., only DTS failing)
  python3 scripts/emit/query-emit.py --close

  # Filter by status
  python3 scripts/emit/query-emit.py --status fail
  python3 scripts/emit/query-emit.py --status timeout

  # Export paths for piping
  python3 scripts/emit/query-emit.py --js-failures --paths-only
"""

import sys
import json
import argparse
from collections import Counter
from pathlib import Path

DETAIL_FILE = Path(__file__).parent / "emit-detail.json"


def load_detail():
    if not DETAIL_FILE.exists():
        print(f"Error: {DETAIL_FILE} not found.")
        print("Run: ./scripts/emit/run.sh --json-out")
        sys.exit(1)
    with open(DETAIL_FILE) as f:
        return json.load(f)


def show_overview(data):
    s = data["summary"]
    print(f"Emit Test Results")
    print(f"  JavaScript: {s['jsPass']}/{s['jsTotal']} ({s['jsPassRate']}%)")
    print(f"  Declaration: {s['dtsPass']}/{s['dtsTotal']} ({s['dtsPassRate']}%)")
    print()

    results = data["results"]
    js_fails = [r for r in results if r["jsStatus"] == "fail"]
    dts_fails = [r for r in results if r["dtsStatus"] == "fail"]
    timeouts = [r for r in results if r["jsStatus"] == "timeout" or r["dtsStatus"] == "timeout"]

    print(f"  JS failures: {len(js_fails)}")
    print(f"  DTS failures: {len(dts_fails)}")
    print(f"  Timeouts: {len(timeouts)}")
    print()

    # JS-pass but DTS-fail (close to full pass)
    js_pass_dts_fail = [r for r in results if r["jsStatus"] == "pass" and r["dtsStatus"] == "fail"]
    print(f"  JS pass + DTS fail (close to full pass): {len(js_pass_dts_fail)}")

    # DTS-pass but JS-fail
    dts_pass_js_fail = [r for r in results if r["dtsStatus"] == "pass" and r["jsStatus"] == "fail"]
    print(f"  DTS pass + JS fail: {len(dts_pass_js_fail)}")
    print()

    # Top error messages
    print("Top JS failure messages:")
    js_error_counter = Counter()
    for r in js_fails:
        msg = r.get("jsError", "unknown")
        # Normalize to first 80 chars
        js_error_counter[msg[:80]] += 1
    for msg, count in js_error_counter.most_common(10):
        print(f"  {count:>4d}  {msg}")
    print()

    print("Top DTS failure messages:")
    dts_error_counter = Counter()
    for r in dts_fails:
        msg = r.get("dtsError", "unknown")
        dts_error_counter[msg[:80]] += 1
    for msg, count in dts_error_counter.most_common(10):
        print(f"  {count:>4d}  {msg}")


def show_js_failures(data, top=40, paths_only=False):
    results = data["results"]
    fails = [r for r in results if r["jsStatus"] == "fail"]
    fails.sort(key=lambda r: r["name"])

    if paths_only:
        for r in fails:
            print(r["name"])
        return

    print(f"JS failures: {len(fails)}")
    for r in fails[:top]:
        err = r.get("jsError", "")[:80]
        print(f"  {r['name']}  {err}")
    if len(fails) > top:
        print(f"  ... and {len(fails) - top} more")


def show_dts_failures(data, top=40, paths_only=False):
    results = data["results"]
    fails = [r for r in results if r["dtsStatus"] == "fail"]
    fails.sort(key=lambda r: r["name"])

    if paths_only:
        for r in fails:
            print(r["name"])
        return

    print(f"DTS failures: {len(fails)}")
    for r in fails[:top]:
        err = r.get("dtsError", "")[:80]
        print(f"  {r['name']}  {err}")
    if len(fails) > top:
        print(f"  ... and {len(fails) - top} more")


def show_top_errors(data, top=20):
    results = data["results"]

    print("Top JS error messages:")
    js_counter = Counter()
    for r in results:
        if r["jsStatus"] == "fail" and r.get("jsError"):
            js_counter[r["jsError"][:100]] += 1
    for msg, count in js_counter.most_common(top):
        print(f"  {count:>4d}  {msg}")

    print()
    print("Top DTS error messages:")
    dts_counter = Counter()
    for r in results:
        if r["dtsStatus"] == "fail" and r.get("dtsError"):
            dts_counter[r["dtsError"][:100]] += 1
    for msg, count in dts_counter.most_common(top):
        print(f"  {count:>4d}  {msg}")


def show_close(data, top=40):
    """Tests where JS passes but DTS fails, or vice versa."""
    results = data["results"]
    close = []
    for r in results:
        js_ok = r["jsStatus"] in ("pass", "skip")
        dts_ok = r["dtsStatus"] in ("pass", "skip")
        if js_ok and not dts_ok:
            close.append(("js-pass/dts-fail", r))
        elif dts_ok and not js_ok:
            close.append(("dts-pass/js-fail", r))

    print(f"Close-to-passing tests: {len(close)}")
    for kind, r in close[:top]:
        err = r.get("jsError") or r.get("dtsError") or ""
        print(f"  [{kind}] {r['name']}  {err[:60]}")
    if len(close) > top:
        print(f"  ... and {len(close) - top} more")


def show_filter(data, pattern, top=40, paths_only=False):
    results = data["results"]
    lower = pattern.lower()
    matches = [r for r in results if lower in r["name"].lower()]

    if paths_only:
        for r in matches:
            print(r["name"])
        return

    passing = sum(1 for r in matches if r["jsStatus"] == "pass" and r["dtsStatus"] in ("pass", "skip"))
    print(f"Tests matching '{pattern}': {len(matches)} ({passing} fully passing)")
    for r in matches[:top]:
        status = f"js={r['jsStatus']} dts={r['dtsStatus']}"
        print(f"  {r['name']}  [{status}]")
    if len(matches) > top:
        print(f"  ... and {len(matches) - top} more")


def show_status(data, status, top=40):
    results = data["results"]
    matches = [r for r in results if r["jsStatus"] == status or r["dtsStatus"] == status]
    print(f"Tests with status '{status}': {len(matches)}")
    for r in matches[:top]:
        st = f"js={r['jsStatus']} dts={r['dtsStatus']}"
        err = r.get("jsError") or r.get("dtsError") or ""
        print(f"  {r['name']}  [{st}]  {err[:60]}")
    if len(matches) > top:
        print(f"  ... and {len(matches) - top} more")


def main():
    parser = argparse.ArgumentParser(description="Query emit test results offline")
    parser.add_argument("--js-failures", action="store_true", help="Show JS failures")
    parser.add_argument("--dts-failures", action="store_true", help="Show DTS failures")
    parser.add_argument("--top-errors", action="store_true", help="Show top error messages")
    parser.add_argument("--close", action="store_true", help="Show close-to-passing tests")
    parser.add_argument("--filter", type=str, help="Filter by substring in test name")
    parser.add_argument("--status", type=str, help="Filter by status (pass/fail/skip/timeout)")
    parser.add_argument("--paths-only", action="store_true", help="Output only test names (for piping)")
    parser.add_argument("--top", type=int, default=40, help="Limit rows shown")
    args = parser.parse_args()

    data = load_detail()

    if args.js_failures:
        show_js_failures(data, args.top, args.paths_only)
    elif args.dts_failures:
        show_dts_failures(data, args.top, args.paths_only)
    elif args.top_errors:
        show_top_errors(data, args.top)
    elif args.close:
        show_close(data, args.top)
    elif args.filter:
        show_filter(data, args.filter, args.top, args.paths_only)
    elif args.status:
        show_status(data, args.status, args.top)
    else:
        show_overview(data)


if __name__ == "__main__":
    main()
