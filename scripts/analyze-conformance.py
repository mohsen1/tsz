#!/usr/bin/env python3
"""Analyze conformance test failures and categorize them for quick wins."""

import sys
import re
from collections import defaultdict, Counter


def main():
    if len(sys.argv) < 2:
        print("Usage: analyze-conformance.py <raw-output-file> [category] [top_n]")
        sys.exit(1)

    tmpfile = sys.argv[1]
    category_filter = sys.argv[2] if len(sys.argv) > 2 and sys.argv[2] else ""
    top_n = int(sys.argv[3]) if len(sys.argv) > 3 and sys.argv[3] else 20

    tests = []
    current = None

    with open(tmpfile) as f:
        for line in f:
            line = line.rstrip()
            m = re.match(r"^FAIL (.+?)(?:\s+\(ERROR: .+\))?$", line)
            if m:
                if current:
                    tests.append(current)
                current = {
                    "path": m.group(1),
                    "expected": [],
                    "actual": [],
                    "options": "",
                }
                continue
            if current:
                m = re.match(r"^\s+expected:\s+\[(.*?)?\]", line)
                if m:
                    codes = m.group(1).strip()
                    current["expected"] = (
                        [c.strip() for c in codes.split(",")] if codes else []
                    )
                    continue
                m = re.match(r"^\s+actual:\s+\[(.*?)?\]", line)
                if m:
                    codes = m.group(1).strip()
                    current["actual"] = (
                        [c.strip() for c in codes.split(",")] if codes else []
                    )
                    continue
                m = re.match(r"^\s+options:\s+(.*)", line)
                if m:
                    current["options"] = m.group(1)
                    continue
                if not line.startswith(" "):
                    tests.append(current)
                    current = None

    if current:
        tests.append(current)

    # Skip tests without data
    tests = [t for t in tests if t["expected"] is not None]

    # Categorize
    false_positives = []
    all_missing = []
    wrong_code = []
    close = []

    for t in tests:
        exp_list = t["expected"]
        act_list = t["actual"]

        if not exp_list and act_list:
            t["category"] = "false-positive"
            t["extra_codes"] = sorted(set(act_list))
            false_positives.append(t)
        elif exp_list and not act_list:
            t["category"] = "all-missing"
            t["missing_codes"] = sorted(set(exp_list))
            all_missing.append(t)
        elif exp_list and act_list:
            exp_counter = Counter(exp_list)
            act_counter = Counter(act_list)
            missing = []
            extra = []
            for code in set(list(exp_counter.keys()) + list(act_counter.keys())):
                diff = act_counter.get(code, 0) - exp_counter.get(code, 0)
                if diff > 0:
                    extra.extend([code] * diff)
                elif diff < 0:
                    missing.extend([code] * (-diff))
            t["missing_codes"] = sorted(missing)
            t["extra_codes"] = sorted(extra)
            t["diff_size"] = len(missing) + len(extra)
            t["category"] = "wrong-code"
            wrong_code.append(t)
            if t["diff_size"] <= 2:
                close.append(t)

    close.sort(key=lambda t: t["diff_size"])

    def basename(p):
        return p.rsplit("/", 1)[-1] if "/" in p else p

    def print_section(title, items, show_fn, limit=None):
        limit = limit or top_n
        print(f"\n{'='*70}")
        print(f"  {title} ({len(items)} tests)")
        print(f"{'='*70}")
        for t in items[:limit]:
            show_fn(t)
        if len(items) > limit:
            print(f"  ... and {len(items) - limit} more")

    def show_fp(t):
        codes = ", ".join(t["extra_codes"])
        print(f"  {basename(t['path'])}")
        print(f"    EXTRA: [{codes}]")

    def show_missing(t):
        codes = ", ".join(t["missing_codes"])
        print(f"  {basename(t['path'])}")
        print(f"    MISSING: [{codes}]")

    def show_wrong(t):
        miss = ", ".join(t["missing_codes"]) if t["missing_codes"] else "-"
        ext = ", ".join(t["extra_codes"]) if t["extra_codes"] else "-"
        print(f"  {basename(t['path'])} (diff={t['diff_size']})")
        print(f"    missing=[{miss}]  extra=[{ext}]")

    def show_close(t):
        miss = ", ".join(t["missing_codes"]) if t["missing_codes"] else "-"
        ext = ", ".join(t["extra_codes"]) if t["extra_codes"] else "-"
        print(f"  {basename(t['path'])} (diff={t['diff_size']})")
        print(f"    expected: [{', '.join(t['expected'])}]")
        print(f"    actual:   [{', '.join(t['actual'])}]")
        print(f"    fix: missing=[{miss}]  extra=[{ext}]")

    # Summary
    total = len(tests)
    print(f"\n{'='*70}")
    print(f"  ANALYSIS SUMMARY")
    print(f"{'='*70}")
    print(f"  Total failing tests analyzed: {total}")
    print(f"  False positives (expected=[], we emit errors):  {len(false_positives)}")
    print(f"  All missing (expected errors, we emit none):    {len(all_missing)}")
    print(f"  Wrong codes (both have errors, codes differ):   {len(wrong_code)}")
    print(f"  Close to passing (diff <= 2 codes):             {len(close)}")

    # Top FP codes
    fp_code_freq = defaultdict(int)
    for t in false_positives:
        for c in t["extra_codes"]:
            fp_code_freq[c] += 1
    if fp_code_freq:
        print(f"\n  Top false-positive error codes (fix = instant wins):")
        for code, count in sorted(fp_code_freq.items(), key=lambda x: -x[1])[:10]:
            print(f"    {code}: {count} tests")

    # Top missing codes
    miss_code_freq = defaultdict(int)
    for t in all_missing:
        for c in t["missing_codes"]:
            miss_code_freq[c] += 1
    if miss_code_freq:
        print(f"\n  Top all-missing error codes (implement = new passes):")
        for code, count in sorted(miss_code_freq.items(), key=lambda x: -x[1])[:10]:
            print(f"    {code}: {count} tests")

    # Top extra codes in wrong-code tests
    wc_extra_freq = defaultdict(int)
    for t in wrong_code:
        for c in t["extra_codes"]:
            wc_extra_freq[c] += 1
    if wc_extra_freq:
        print(f"\n  Top extra codes in wrong-code tests (fix = closer to passing):")
        for code, count in sorted(wc_extra_freq.items(), key=lambda x: -x[1])[:10]:
            print(f"    {code}: {count} tests")

    # Print sections based on filter
    if not category_filter or category_filter == "false-positive":
        print_section(
            "FALSE POSITIVES -- expected no errors, we emit errors",
            false_positives,
            show_fp,
        )

    if not category_filter or category_filter == "all-missing":
        print_section(
            "ALL MISSING -- expected errors, we emit nothing", all_missing, show_missing
        )

    if not category_filter or category_filter == "close":
        print_section(
            "CLOSE TO PASSING -- differ by 1-2 error codes", close, show_close
        )

    if category_filter == "wrong-code":
        print_section(
            "WRONG CODES -- both have errors but codes differ", wrong_code, show_wrong
        )

    print()


if __name__ == "__main__":
    main()
