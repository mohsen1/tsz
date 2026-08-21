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
    tests, _identity_counts = _parse_runner_output(path)
    return tests


def _parse_runner_output(path):
    tests = {}
    identity_counts = Counter()
    current_path = None
    current_rec = None

    with open(path) as f:
        for line in f:
            line = line.rstrip("\r\n")

            # PASS / SKIP / UNSUPPORTED / CRASH — single-line, no indented follow-up
            m = re.match(
                r"^(PASS|SKIP|UNSUPPORTED|CRASH)\s+(.+?)(?:\s+\((.+)\))?$",
                line,
            )
            if m:
                status, test_path = m.group(1), m.group(2)
                identity_counts[normalize_harness_path(test_path)] += 1
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
                identity_counts[normalize_harness_path(test_path)] += 1
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
                identity_counts[normalize_harness_path(test_path)] += 1
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

    return tests, identity_counts


def summarize_runner_output(path):
    """Return candidate/runnable accounting from one runner output file."""
    tests, identity_counts = _parse_runner_output(path)
    status_counts = Counter(record["status"] for record in tests.values())

    with open(path, encoding="utf-8", errors="replace") as f:
        text = f.read()

    final_matches = re.findall(
        r"FINAL RESULTS:\s+(\d+)/(\d+)\s+passed\s+\(([0-9.]+)%\)",
        text,
    )
    final = final_matches[0] if len(final_matches) == 1 else None

    def reported_count(label):
        prefix = r"(?:⏱️\s*)?" if label == "Timeout" else ""
        suffix = r"(?:\s+\(.+\))?" if label == "Timeout" else ""
        matches = re.findall(
            rf"^\s*{prefix}{re.escape(label)}:\s*(\d+){suffix}\s*$",
            text,
            re.MULTILINE,
        )
        return int(matches[0]) if len(matches) == 1 else None

    passed = int(final[0]) if final else status_counts["PASS"]
    final_runnable = int(final[1]) if final else None
    reported_runnable = reported_count("Runnable")
    runnable = reported_runnable
    if runnable is None:
        runnable = final_runnable
    if runnable is None:
        runnable = sum(
            status_counts[status]
            for status in ("PASS", "FAIL", "XFAIL", "CRASH", "TIMEOUT")
        )

    reported_unsupported = reported_count("Unsupported")
    unsupported = reported_unsupported
    if unsupported is None:
        unsupported = status_counts["UNSUPPORTED"]
    reported_skipped = reported_count("Skipped")
    skipped = reported_skipped
    if skipped is None:
        skipped = status_counts["SKIP"]
    reported_candidates = reported_count("Candidates")
    candidates = reported_candidates
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
        "status_passed": status_counts["PASS"],
        "failed": runnable - passed,
        "diagnostic_failed": status_counts["FAIL"] + status_counts["XFAIL"],
        "crashed": status_counts["CRASH"],
        "timeout": status_counts["TIMEOUT"],
        "unsupported": unsupported,
        "skipped": skipped,
        "known_failures": status_counts["XFAIL"],
        "reported_crashed": reported_count("Crashed"),
        "reported_timeout": reported_count("Timeout"),
        "reported_known_failures": reported_count("Known failures"),
        "rate": float(final[2]) if final else 0.0,
        "recorded": recorded_candidates,
        "recorded_candidates": recorded_candidates,
        "recorded_runnable": recorded_runnable,
        "has_final_results": final is not None,
        "final_results_count": len(final_matches),
        "final_runnable": final_runnable,
        "reported_candidates": reported_candidates,
        "reported_runnable": reported_runnable,
        "reported_unsupported": reported_unsupported,
        "reported_skipped": reported_skipped,
        "duplicate_identities": sorted(
            identity for identity, count in identity_counts.items() if count != 1
        ),
        "partition_valid": candidates == runnable + unsupported + skipped,
    }


def validate_runner_summary(summary, runner_status=None):
    """Fail closed unless one runner observation is a complete result bijection."""
    errors = []
    if summary["final_results_count"] != 1:
        errors.append("runner output must contain exactly one FINAL RESULTS summary")
    for key, label in (
        ("reported_candidates", "Candidates"),
        ("reported_runnable", "Runnable"),
        ("reported_unsupported", "Unsupported"),
        ("reported_skipped", "Skipped"),
    ):
        if summary[key] is None:
            errors.append(f"runner output must contain exactly one {label} count")
    if summary["duplicate_identities"]:
        errors.append(
            "runner output repeats terminal identities: "
            + ", ".join(summary["duplicate_identities"][:3])
        )
    if summary["candidates"] <= 0:
        errors.append("runner selected no candidates")
    if not summary["partition_valid"]:
        errors.append("candidate partition arithmetic is inconsistent")
    if summary["recorded_candidates"] != summary["candidates"]:
        errors.append("candidate identities do not cover the reported selection")
    if summary["recorded_runnable"] != summary["runnable"]:
        errors.append("runnable identities do not cover the reported denominator")
    if summary["final_runnable"] != summary["runnable"]:
        errors.append("FINAL RESULTS denominator differs from Runnable")
    if summary["passed"] != summary.get("status_passed", summary["passed"]):
        errors.append("FINAL RESULTS pass count differs from PASS identities")
    if summary["failed"] != (
        summary["diagnostic_failed"] + summary["crashed"] + summary["timeout"]
    ):
        errors.append("runnable terminal status arithmetic is inconsistent")
    for recorded, reported, label in (
        (summary["crashed"], summary["reported_crashed"], "crashed"),
        (summary["timeout"], summary["reported_timeout"], "timeout"),
        (
            summary["known_failures"],
            summary["reported_known_failures"],
            "known failures",
        ),
    ):
        if reported is None or recorded != reported:
            errors.append(f"{label} identities differ from the reported count")
    expected_rate = 0.0
    if summary["runnable"]:
        expected_rate = round(100.0 * summary["passed"] / summary["runnable"], 1)
    if summary["rate"] != expected_rate:
        errors.append("FINAL RESULTS pass rate is arithmetically inconsistent")
    terminal_failures = summary["failed"] > 0
    if runner_status is not None:
        expected_status = 1 if terminal_failures else 0
        if runner_status != expected_status:
            errors.append(
                f"runner exit status must be exactly {expected_status} for these terminal results"
            )
        summary["runner_status"] = runner_status
    if errors:
        raise ValueError("; ".join(errors))
    return summary


def require_complete_runner_summary(path, runner_status=None):
    summary = summarize_runner_output(path)
    return validate_runner_summary(summary, runner_status)


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
