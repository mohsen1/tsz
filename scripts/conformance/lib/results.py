#!/usr/bin/env python3
"""Shared helpers for parsing conformance runner output.

Provides a single canonical parser for the raw line-oriented output produced
by ``conformance.sh`` and related tools, plus a ``compute_diff`` helper used
by multiple analysis scripts.
"""

import re
from collections import Counter


_ABS_HARNESS_PREFIX_RE = re.compile(r"^.*(TypeScript/)")


def normalize_harness_path(path):
    """Normalize absolute harness paths to start at the in-repo TypeScript/ root.

    The match is intentionally greedy: if a checkout path itself contains
    ``TypeScript/`` (for example
    ``/workspace/TypeScript/tsz/TypeScript/tests/cases/foo.ts``), the final
    occurrence is the in-repo harness path.
    """
    return _ABS_HARNESS_PREFIX_RE.sub(r"\1", path, count=1)


def parse_runner_output(path):
    """Parse raw conformance runner output into per-test records.

    Returns a dict mapping test_path -> record, where each record has:
      status:             str        (PASS | FAIL | XFAIL | SKIP | UNSUPPORTED |
                                      CRASH | TIMEOUT)
      expected:           list[str]  (error codes; empty list when not present)
      actual:             list[str]  (error codes; empty list when not present)
      options:            str        (compiler options string, empty when absent)
      known_failure:      str        (XFAIL reason; empty string when absent)
      unsupported_reason: str        (UNSUPPORTED reason; empty otherwise)

    All test paths are preserved as they appear in the runner output.
    PASS/SKIP/UNSUPPORTED/CRASH/TIMEOUT records always have empty
    expected/actual lists.
    """
    tests = {}
    current_path = None
    current_rec = None

    with open(path) as f:
        for line in f:
            line = line.rstrip()

            # PASS / SKIP / UNSUPPORTED / CRASH — single-line, no indented follow-up
            m = re.match(
                r"^(PASS|SKIP|UNSUPPORTED|CRASH)\s+(.+?)(?:\s+\((.+)\))?$",
                line,
            )
            if m:
                status, test_path = m.group(1), m.group(2)
                tests[test_path] = {
                    "status": status,
                    "expected": [],
                    "actual": [],
                    "options": "",
                    "known_failure": "",
                    "unsupported_reason": (
                        m.group(3) if status == "UNSUPPORTED" and m.group(3) else ""
                    ),
                }
                current_path = None
                current_rec = None
                continue

            # TIMEOUT — plain and emoji-prefix variants
            m = re.match(r"^(?:⏱️\s+)?TIMEOUT\s+(.+?)(?:\s+\(.+\))?$", line)
            if m:
                test_path = m.group(1)
                tests[test_path] = {
                    "status": "TIMEOUT",
                    "expected": [],
                    "actual": [],
                    "options": "",
                    "known_failure": "",
                    "unsupported_reason": "",
                }
                current_path = None
                current_rec = None
                continue

            # FAIL / XFAIL — followed by indented expected/actual/options lines
            m = re.match(r"^(FAIL|XFAIL)\s+(.+?)(?:\s+\((.+)\))?$", line)
            if m:
                status, test_path = m.group(1), m.group(2)
                known_failure = m.group(3) if status == "XFAIL" and m.group(3) else ""
                current_rec = {
                    "status": status,
                    "expected": [],
                    "actual": [],
                    "options": "",
                    "known_failure": known_failure,
                    "unsupported_reason": "",
                }
                current_path = test_path
                tests[test_path] = current_rec
                continue

            # Indented detail lines that follow a FAIL/XFAIL record
            if current_path and current_rec:
                m = re.match(r"^\s+expected:\s+\[(.*?)?\]", line)
                if m:
                    codes = m.group(1).strip() if m.group(1) else ""
                    current_rec["expected"] = [c.strip() for c in codes.split(",") if c.strip()]
                    continue
                m = re.match(r"^\s+actual:\s+\[(.*?)?\]", line)
                if m:
                    codes = m.group(1).strip() if m.group(1) else ""
                    current_rec["actual"] = [c.strip() for c in codes.split(",") if c.strip()]
                    continue
                m = re.match(r"^\s+options:\s+(.*)", line)
                if m:
                    current_rec["options"] = m.group(1)
                    continue
                # A non-indented line terminates the current FAIL block
                if not line.startswith(" "):
                    current_path = None
                    current_rec = None

    return tests


def summarize_runner_output(path):
    """Return candidate/runnable accounting from one runner output file."""
    tests = parse_runner_output(path)
    status_counts = Counter(record["status"] for record in tests.values())

    with open(path, encoding="utf-8", errors="replace") as f:
        text = f.read()

    final = re.search(
        r"FINAL RESULTS:\s+(\d+)/(\d+)\s+passed\s+\(([0-9.]+)%\)",
        text,
    )

    def reported_count(label):
        matches = re.findall(rf"^\s*{re.escape(label)}:\s*(\d+)\s*$", text, re.MULTILINE)
        return int(matches[-1]) if matches else None

    passed = int(final.group(1)) if final else status_counts["PASS"]
    final_runnable = int(final.group(2)) if final else None
    runnable = reported_count("Runnable")
    if runnable is None:
        runnable = final_runnable
    if runnable is None:
        runnable = sum(
            status_counts[status]
            for status in ("PASS", "FAIL", "XFAIL", "CRASH", "TIMEOUT")
        )

    unsupported = reported_count("Unsupported")
    if unsupported is None:
        unsupported = status_counts["UNSUPPORTED"]
    skipped = reported_count("Skipped")
    if skipped is None:
        skipped = status_counts["SKIP"]
    candidates = reported_count("Candidates")
    if candidates is None:
        candidates = runnable + unsupported + skipped

    recorded_runnable = sum(
        status_counts[status]
        for status in ("PASS", "FAIL", "XFAIL", "CRASH", "TIMEOUT")
    )
    recorded_candidates = len(tests)

    return {
        # `total` remains the legacy name for the runnable denominator.
        "total": runnable,
        "runnable": runnable,
        "candidates": candidates,
        "passed": passed,
        "failed": runnable - passed,
        "unsupported": unsupported,
        "skipped": skipped,
        "rate": float(final.group(3)) if final else 0.0,
        "recorded": recorded_candidates,
        "recorded_candidates": recorded_candidates,
        "recorded_runnable": recorded_runnable,
        "has_final_results": final is not None,
        "partition_valid": candidates == runnable + unsupported + skipped,
    }


def compute_diff(expected, actual):
    """Return (missing, extra) code lists comparing expected vs actual.

    missing: codes present in expected but absent (or under-represented) in actual
    extra:   codes present in actual but absent (or over-represented) in expected

    Both lists are sorted and may contain duplicates when a code appears multiple
    times with a count mismatch.
    """
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
