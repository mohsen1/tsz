#!/usr/bin/env python3
"""Analyze conformance test failures and categorize them for quick wins."""

import sys
import os
import json
import argparse
from collections import defaultdict
from itertools import combinations

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib.results import parse_runner_output, compute_diff


def main():
    parser = argparse.ArgumentParser(description='Analyze conformance test failures')
    parser.add_argument('input_file', help='Conformance runner output file')
    parser.add_argument('--category', default='', help='Filter by category')
    parser.add_argument('--top', type=int, default=20, help='Top N items per section')
    parser.add_argument('--json-output', default='', help='Write structured JSON to this path')
    args = parser.parse_args()
    tmpfile = args.input_file
    category_filter = args.category
    top_n = args.top

    raw = parse_runner_output(tmpfile)
    tests = [
        {**rec, "path": path}
        for path, rec in raw.items()
        if rec["status"] in ("FAIL", "XFAIL")
    ]

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
            missing, extra = compute_diff(exp_list, act_list)
            t["missing_codes"] = missing
            t["extra_codes"] = extra
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

    # ============================================================================
    # IMPACT ANALYSIS: Missing error codes that would pass the most tests
    # ============================================================================
    
    # Collect all error codes ever emitted by tsz
    all_actual_codes = set()
    for t in tests:
        all_actual_codes.update(t["actual"])
    
    # Collect all expected error codes
    all_expected_codes = set()
    for t in tests:
        all_expected_codes.update(t["expected"])
    
    # Find codes that are expected but NEVER emitted by tsz
    completely_missing_codes = all_expected_codes - all_actual_codes
    
    # For each completely missing code, count how many tests it appears in
    missing_code_impact = defaultdict(int)
    for t in tests:
        for code in t["expected"]:
            if code in completely_missing_codes:
                missing_code_impact[code] += 1
    
    # Find codes that we emit but in wrong places (over-emit or misplace)
    wrongly_emitted_codes = set()
    for t in false_positives + wrong_code:
        wrongly_emitted_codes.update(t.get("extra_codes", []))
    
    # Codes we emit correctly sometimes but miss in other tests
    partially_implemented_codes = all_expected_codes & all_actual_codes
    partially_missing_impact = defaultdict(int)
    for t in all_missing + wrong_code:
        for code in t.get("missing_codes", []):
            if code in partially_implemented_codes:
                partially_missing_impact[code] += 1
    
    print(f"\n{'='*70}")
    print(f"  🎯 IMPACT ANALYSIS: Error Code Implementation Priority")
    print(f"{'='*70}")
    
    if missing_code_impact:
        print(f"\n  🔴 NOT IMPLEMENTED (never emitted by tsz):")
        print(f"     Implementing these will have immediate impact!")
        sorted_missing = sorted(missing_code_impact.items(), key=lambda x: -x[1])[:15]
        for code, count in sorted_missing:
            print(f"     {code:6s} → appears in {count:3d} failing tests")
        if len(missing_code_impact) > 15:
            total_impact = sum(missing_code_impact.values())
            shown_impact = sum(count for _, count in sorted_missing)
            print(f"     ... and {len(missing_code_impact)-15} more codes affecting {total_impact-shown_impact} tests")
    
    if partially_missing_impact:
        print(f"\n  🟡 PARTIALLY IMPLEMENTED (emitted sometimes, missing others):")
        print(f"     These work in some cases but need broader coverage.")
        sorted_partial = sorted(partially_missing_impact.items(), key=lambda x: -x[1])[:15]
        for code, count in sorted_partial:
            print(f"     {code:6s} → missing in {count:3d} tests")
        if len(partially_missing_impact) > 15:
            print(f"     ... and {len(partially_missing_impact)-15} more codes")
    
    if wrongly_emitted_codes:
        over_emit_freq = defaultdict(int)
        for t in false_positives + wrong_code:
            for c in t.get("extra_codes", []):
                over_emit_freq[c] += 1
        print(f"\n  🟠 FALSELY EMITTED (emitted when shouldn't be):")
        print(f"     Fixing these reduces false positives.")
        sorted_wrong = sorted(over_emit_freq.items(), key=lambda x: -x[1])[:10]
        for code, count in sorted_wrong:
            impl_status = "✓" if code in all_actual_codes else "✗"
            print(f"     {code:6s} → incorrectly emitted in {count:3d} tests")
    
    # ============================================================================
    # CO-OCCURRENCE ANALYSIS: Error codes that appear together
    # ============================================================================
    
    # Find pairs of error codes that commonly appear together in expected output
    code_pairs = defaultdict(int)
    code_triples = defaultdict(int)
    
    for t in all_missing:
        codes = set(t["missing_codes"])
        if len(codes) >= 2:
            for pair in combinations(sorted(codes), 2):
                code_pairs[pair] += 1
        if len(codes) >= 3:
            for triple in combinations(sorted(codes), 3):
                code_triples[triple] += 1
    
    if code_pairs:
        print(f"\n{'='*70}")
        print(f"  🔗 CO-OCCURRENCE ANALYSIS: Error Codes That Appear Together")
        print(f"{'='*70}")
        print(f"     Implementing these groups will pass multiple tests at once!")
        
        # Show top code pairs
        print(f"\n  Top error code PAIRS (appear together in same test):")
        sorted_pairs = sorted(code_pairs.items(), key=lambda x: -x[1])[:10]
        for pair, count in sorted_pairs:
            code1, code2 = pair
            status1 = "✗" if code1 in completely_missing_codes else "✓"
            status2 = "✗" if code2 in completely_missing_codes else "✓"
            print(f"     {status1} {code1:6s} + {status2} {code2:6s} → {count:2d} tests")
    
    if code_triples:
        print(f"\n  Top error code TRIPLES (three codes together):")
        sorted_triples = sorted(code_triples.items(), key=lambda x: -x[1])[:8]
        for triple, count in sorted_triples:
            code1, code2, code3 = triple
            status1 = "✗" if code1 in completely_missing_codes else "✓"
            status2 = "✗" if code2 in completely_missing_codes else "✓"
            status3 = "✗" if code3 in completely_missing_codes else "✓"
            print(f"     {status1} {code1:6s} + {status2} {code2:6s} + {status3} {code3:6s} → {count:2d} tests")
    
    # ============================================================================
    # QUICK WINS: Single-code tests
    # ============================================================================
    
    single_code_tests = [t for t in all_missing if len(t["missing_codes"]) == 1]
    single_code_freq = defaultdict(int)
    if single_code_tests:
        for t in single_code_tests:
            code = t["missing_codes"][0]
            single_code_freq[code] += 1
        
        print(f"\n{'='*70}")
        print(f"  ⚡ QUICK WINS: Tests Missing Just ONE Error Code")
        print(f"{'='*70}")
        print(f"     Total single-code tests: {len(single_code_tests)}")
        print(f"     Implementing these codes = instant test passes!\n")
        
        sorted_single = sorted(single_code_freq.items(), key=lambda x: -x[1])[:10]
        for code, count in sorted_single:
            impl_status = "NOT IMPL" if code in completely_missing_codes else "partial"
            print(f"     {code:6s} ({impl_status:8s}) → {count:2d} tests would pass")

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

    if args.json_output:
        data = {
            "summary": {
                "total_failing": total,
                "false_positives": len(false_positives),
                "all_missing": len(all_missing),
                "wrong_code": len(wrong_code),
                "close": len(close),
            },
            "not_implemented_codes": [
                {"code": code, "count": count}
                for code, count in sorted(missing_code_impact.items(), key=lambda x: -x[1])[:20]
            ] if missing_code_impact else [],
            "partial_codes": [
                {"code": code, "count": count}
                for code, count in sorted(partially_missing_impact.items(), key=lambda x: -x[1])[:20]
            ] if partially_missing_impact else [],
            "quick_wins": [
                {"code": code, "count": count}
                for code, count in sorted(single_code_freq.items(), key=lambda x: -x[1])[:20]
            ] if single_code_tests else [],
        }
        with open(args.json_output, 'w') as f:
            json.dump(data, f, indent=2)

    print()


if __name__ == "__main__":
    main()
