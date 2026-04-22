#!/usr/bin/env python3
"""Parse conformance runner output into a structured per-test detail JSON file.

This runs as part of `conformance.sh snapshot` and produces
`scripts/conformance/conformance-detail.json` — a compact file that enables all offline
analysis (1-missing tests, false positives, code co-occurrence, etc.) without
re-running the full conformance suite.

Input:  raw runner output (--print-test mode) on stdin or as a file argument
Output: JSON file with per-test results and pre-computed aggregates
"""

import sys
import re
import json
import argparse
from collections import Counter, defaultdict


def parse_runner_output(path):
    """Parse the raw runner output into per-test records."""
    tests = {}
    current_path = None
    current_status = None

    with open(path) as f:
        for line in f:
            line = line.rstrip()

            # PASS line
            m = re.match(r"^PASS\s+(.+?)$", line)
            if m:
                test_path = m.group(1)
                tests[test_path] = {"status": "PASS"}
                current_path = None
                continue

            # FAIL/XFAIL line
            m = re.match(r"^(FAIL|XFAIL)\s+(.+?)(?:\s+\((.+)\))?$", line)
            if m:
                current_path = m.group(2)
                current_status = {"status": m.group(1), "expected": [], "actual": []}
                if m.group(1) == "XFAIL" and m.group(3):
                    current_status["known_failure"] = m.group(3)
                tests[current_path] = current_status
                continue

            # expected/actual lines (indented, after a FAIL)
            if current_path and current_status:
                m = re.match(r"^\s+expected:\s+\[(.*?)?\]", line)
                if m:
                    codes = m.group(1).strip() if m.group(1) else ""
                    current_status["expected"] = [c.strip() for c in codes.split(",") if c.strip()]
                    continue
                m = re.match(r"^\s+actual:\s+\[(.*?)?\]", line)
                if m:
                    codes = m.group(1).strip() if m.group(1) else ""
                    current_status["actual"] = [c.strip() for c in codes.split(",") if c.strip()]
                    continue
                # Non-indented line means end of this FAIL block
                if not line.startswith(" "):
                    current_path = None
                    current_status = None

    return tests


def compute_diff(expected, actual):
    """Compute missing/extra codes between expected and actual."""
    exp_counter = Counter(expected)
    act_counter = Counter(actual)
    missing = []
    extra = []
    for code in set(list(exp_counter.keys()) + list(act_counter.keys())):
        diff = act_counter.get(code, 0) - exp_counter.get(code, 0)
        if diff > 0:
            extra.extend([code] * diff)
        elif diff < 0:
            missing.extend([code] * (-diff))
    return sorted(missing), sorted(extra)


def build_aggregates(tests):
    """Build pre-computed aggregate data from per-test results."""
    # Counters
    one_missing_zero_extra = Counter()  # code -> count of tests fixable by adding just this code
    one_extra_zero_missing = Counter()  # code -> count of tests fixable by removing just this code
    false_positive_codes = Counter()    # code -> count in tests where expected=[] but we emit
    all_missing_codes = Counter()       # code -> count in tests where actual=[] but expected != []
    missing_codes_global = Counter()    # code -> total count across all failing tests
    extra_codes_global = Counter()      # code -> total count across all failing tests

    # Sets for implementation status
    all_emitted = set()
    all_expected = set()

    # Category counts
    n_false_positive = 0
    n_all_missing = 0
    n_wrong_code = 0
    n_fingerprint_only = 0
    n_same_code_count_drift = 0
    n_close = 0  # diff <= 2

    fail_tests = {}

    for path, result in tests.items():
        if result["status"] not in ("FAIL", "XFAIL"):
            continue

        expected = result.get("expected", [])
        actual = result.get("actual", [])
        exp_counter = Counter(expected)
        act_counter = Counter(actual)
        missing, extra = compute_diff(expected, actual)

        all_emitted.update(actual)
        all_expected.update(expected)

        for c in missing:
            missing_codes_global[c] += 1
        for c in extra:
            extra_codes_global[c] += 1

        # Categorize
        if not expected and actual:
            n_false_positive += 1
            for c in set(actual):
                false_positive_codes[c] += 1
        elif expected and not actual:
            n_all_missing += 1
            for c in set(expected):
                all_missing_codes[c] += 1
        elif expected and actual:
            diff_size = len(missing) + len(extra)
            if diff_size == 0:
                n_fingerprint_only += 1
            else:
                if set(exp_counter) == set(act_counter):
                    n_same_code_count_drift += 1
                n_wrong_code += 1
                if diff_size <= 2:
                    n_close += 1

        # 1-missing-0-extra
        if len(missing) == 1 and len(extra) == 0:
            one_missing_zero_extra[missing[0]] += 1

        # 0-missing-1-extra
        if len(missing) == 0 and len(extra) == 1:
            one_extra_zero_missing[extra[0]] += 1

        fail_tests[path] = {
            "expected": expected,
            "actual": actual,
            "missing": missing,
            "extra": extra,
        }

    not_implemented = all_expected - all_emitted
    not_impl_impact = Counter()
    for path, ft in fail_tests.items():
        for c in ft["missing"]:
            if c in not_implemented:
                not_impl_impact[c] += 1

    partial_impl = all_expected & all_emitted
    partial_missing_impact = Counter()
    for path, ft in fail_tests.items():
        for c in ft["missing"]:
            if c in partial_impl:
                partial_missing_impact[c] += 1

    return {
        "categories": {
            "false_positive": n_false_positive,
            "all_missing": n_all_missing,
            "wrong_code": n_wrong_code,
            "fingerprint_only": n_fingerprint_only,
            "same_code_count_drift": n_same_code_count_drift,
            "close_to_passing": n_close,
        },
        "one_missing_zero_extra": [
            {"code": code, "count": count}
            for code, count in one_missing_zero_extra.most_common(50)
        ],
        "one_extra_zero_missing": [
            {"code": code, "count": count}
            for code, count in one_extra_zero_missing.most_common(50)
        ],
        "false_positive_codes": [
            {"code": code, "count": count}
            for code, count in false_positive_codes.most_common(30)
        ],
        "all_missing_codes": [
            {"code": code, "count": count}
            for code, count in all_missing_codes.most_common(30)
        ],
        "not_implemented_codes": [
            {"code": code, "count": count}
            for code, count in not_impl_impact.most_common(30)
        ],
        "partial_codes": [
            {"code": code, "count": count}
            for code, count in partial_missing_impact.most_common(30)
        ],
        "top_missing_codes": [
            {"code": code, "count": count}
            for code, count in missing_codes_global.most_common(40)
        ],
        "top_extra_codes": [
            {"code": code, "count": count}
            for code, count in extra_codes_global.most_common(40)
        ],
    }


def main():
    parser = argparse.ArgumentParser(description="Build conformance detail snapshot")
    parser.add_argument("input_file", help="Raw runner output file (--print-test mode)")
    parser.add_argument("--output", "-o", required=True, help="Output JSON path")
    args = parser.parse_args()

    tests = parse_runner_output(args.input_file)
    aggregates = build_aggregates(tests)

    total = len(tests)
    passed = sum(1 for t in tests.values() if t["status"] == "PASS")
    failed = total - passed

    # Build compact per-test detail: only store non-passing tests (PASS is implicit)
    fail_detail = {}
    for path, result in sorted(tests.items()):
        if result["status"] not in ("FAIL", "XFAIL"):
            continue
        expected = result.get("expected", [])
        actual = result.get("actual", [])
        missing, extra = compute_diff(expected, actual)
        entry = {}
        if expected:
            entry["e"] = expected
        if actual:
            entry["a"] = actual
        if missing:
            entry["m"] = missing
        if extra:
            entry["x"] = extra
        if result["status"] == "XFAIL":
            entry["status"] = "XFAIL"
            if reason := result.get("known_failure"):
                entry["reason"] = reason
        fail_detail[path] = entry

    output = {
        "summary": {
            "total": total,
            "passed": passed,
            "failed": failed,
            "known_failures": sum(1 for t in tests.values() if t["status"] == "XFAIL"),
        },
        "aggregates": aggregates,
        "failures": fail_detail,
    }

    with open(args.output, "w") as f:
        json.dump(output, f, separators=(",", ":"))

    size_kb = len(json.dumps(output, separators=(",", ":"))) / 1024
    print(f"Detail snapshot: {total} tests, {passed} passed, {failed} failed ({size_kb:.0f} KB)")


if __name__ == "__main__":
    main()
