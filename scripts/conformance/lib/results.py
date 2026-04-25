#!/usr/bin/env python3
"""Shared helpers for parsing conformance runner output.

Provides a single canonical parser for the raw line-oriented output produced
by ``conformance.sh`` and related tools, plus a ``compute_diff`` helper used
by multiple analysis scripts.
"""

import re
from collections import Counter


def parse_runner_output(path):
    """Parse raw conformance runner output into per-test records.

    Returns a dict mapping test_path -> record, where each record has:
      status:        str        (PASS | FAIL | XFAIL | SKIP | CRASH | TIMEOUT)
      expected:      list[str]  (error codes; empty list when not present)
      actual:        list[str]  (error codes; empty list when not present)
      options:       str        (compiler options string, empty string when absent)
      known_failure: str        (XFAIL reason; empty string when absent)

    All test paths are preserved as they appear in the runner output.
    PASS/SKIP/CRASH/TIMEOUT records always have empty expected/actual lists.
    """
    tests = {}
    current_path = None
    current_rec = None

    with open(path) as f:
        for line in f:
            line = line.rstrip()

            # PASS / SKIP / CRASH — single-line, no indented follow-up
            m = re.match(r"^(PASS|SKIP|CRASH)\s+(.+?)(?:\s+\(.+\))?$", line)
            if m:
                status, test_path = m.group(1), m.group(2)
                tests[test_path] = {
                    "status": status,
                    "expected": [],
                    "actual": [],
                    "options": "",
                    "known_failure": "",
                }
                current_path = None
                current_rec = None
                continue

            # TIMEOUT — emoji prefix variant
            m = re.match(r"^⏱️\s+TIMEOUT\s+(.+?)(?:\s+\(.+\))?$", line)
            if m:
                test_path = m.group(1)
                tests[test_path] = {
                    "status": "TIMEOUT",
                    "expected": [],
                    "actual": [],
                    "options": "",
                    "known_failure": "",
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
