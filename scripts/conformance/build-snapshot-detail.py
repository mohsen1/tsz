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
import os
import json
import argparse
from collections import Counter, defaultdict

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib.results import parse_runner_output, compute_diff


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


def build_snapshot_detail(tests, git_sha=None):
    """Build the compact detail artifact with explicit runnable accounting."""
    aggregates = build_aggregates(tests)

    candidates = len(tests)
    passed = sum(1 for result in tests.values() if result["status"] == "PASS")
    unsupported = sum(
        1 for result in tests.values() if result["status"] == "UNSUPPORTED"
    )
    skipped = sum(1 for result in tests.values() if result["status"] == "SKIP")
    runnable = candidates - unsupported - skipped
    failed = runnable - passed

    # Build compact per-test detail: only store runnable failures. PASS is
    # implicit, while unsupported candidates are recorded separately with a
    # stable reason so they cannot be mistaken for either failures or skips.
    fail_detail = {}
    unsupported_detail = {}
    for path, result in sorted(tests.items()):
        if result["status"] == "UNSUPPORTED":
            unsupported_detail[path] = {
                "reason": result.get("unsupported_reason", ""),
            }
            continue
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

    detail = {
        "summary": {
            "candidates": candidates,
            # `total` remains the backward-compatible pass denominator.
            "total": runnable,
            "runnable": runnable,
            "passed": passed,
            "failed": failed,
            "unsupported": unsupported,
            "skipped": skipped,
            "known_failures": sum(
                1 for result in tests.values() if result["status"] == "XFAIL"
            ),
        },
        "aggregates": aggregates,
        "failures": fail_detail,
        "unsupported": unsupported_detail,
    }
    # Stamp the measured tree so observational artifacts retain provenance.
    if git_sha and git_sha.lower() != "unknown":
        detail["git_sha"] = git_sha
    return detail


def main():
    parser = argparse.ArgumentParser(description="Build conformance detail snapshot")
    parser.add_argument("input_file", help="Raw runner output file (--print-test mode)")
    parser.add_argument("--output", "-o", required=True, help="Output JSON path")
    parser.add_argument(
        "--git-sha",
        default=None,
        help="commit SHA the runner output was measured against; stamped into "
        "the artifact so consumers can distinguish a current reading "
        "from a stale local snapshot",
    )
    args = parser.parse_args()

    tests = parse_runner_output(args.input_file)
    output = build_snapshot_detail(tests, git_sha=args.git_sha)

    with open(args.output, "w") as f:
        json.dump(output, f, separators=(",", ":"))

    size_kb = len(json.dumps(output, separators=(",", ":"))) / 1024
    summary = output["summary"]
    print(
        "Detail snapshot: "
        f"{summary['candidates']} candidates, {summary['runnable']} runnable, "
        f"{summary['passed']} passed, {summary['failed']} failed, "
        f"{summary['unsupported']} unsupported, {summary['skipped']} skipped "
        f"({size_kb:.0f} KB)"
    )


if __name__ == "__main__":
    main()
