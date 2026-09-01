#!/usr/bin/env python3
"""Unit tests for scripts/conformance/lib/results.py.

Run directly:  python3 scripts/conformance/lib/test_results.py
"""

import io
import sys
import os
import pathlib
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
from lib.results import (
    compute_diff,
    normalize_harness_path,
    parse_runner_output,
    require_complete_runner_summary,
    summarize_runner_output,
)


def _write_tmp(content):
    """Write content to a temporary file and return the path."""
    f = tempfile.NamedTemporaryFile(mode="w", suffix=".txt", delete=False)
    f.write(content)
    f.flush()
    f.close()
    return f.name


class TestComputeDiff(unittest.TestCase):
    def test_identical(self):
        missing, extra = compute_diff(["TS2322"], ["TS2322"])
        self.assertEqual(missing, [])
        self.assertEqual(extra, [])

    def test_missing_one(self):
        missing, extra = compute_diff(["TS2322", "TS2345"], ["TS2322"])
        self.assertEqual(missing, ["TS2345"])
        self.assertEqual(extra, [])

    def test_extra_one(self):
        missing, extra = compute_diff(["TS2322"], ["TS2322", "TS2345"])
        self.assertEqual(missing, [])
        self.assertEqual(extra, ["TS2345"])

    def test_both_empty(self):
        missing, extra = compute_diff([], [])
        self.assertEqual(missing, [])
        self.assertEqual(extra, [])

    def test_expected_empty_actual_has_codes(self):
        missing, extra = compute_diff([], ["TS2322"])
        self.assertEqual(missing, [])
        self.assertEqual(extra, ["TS2322"])

    def test_expected_has_codes_actual_empty(self):
        missing, extra = compute_diff(["TS2322"], [])
        self.assertEqual(missing, ["TS2322"])
        self.assertEqual(extra, [])

    def test_duplicate_count_mismatch(self):
        # Expected has two TS2322; actual has one → one missing
        missing, extra = compute_diff(["TS2322", "TS2322"], ["TS2322"])
        self.assertEqual(missing, ["TS2322"])
        self.assertEqual(extra, [])

    def test_results_are_sorted(self):
        missing, extra = compute_diff(["TS2345", "TS2322"], ["TS2339"])
        self.assertEqual(missing, ["TS2322", "TS2345"])
        self.assertEqual(extra, ["TS2339"])


class TestNormalizeHarnessPath(unittest.TestCase):
    def test_relative_path_is_unchanged(self):
        self.assertEqual(
            normalize_harness_path("TypeScript/tests/cases/compiler/foo.ts"),
            "TypeScript/tests/cases/compiler/foo.ts",
        )

    def test_absolute_path_strips_to_typescript_root(self):
        self.assertEqual(
            normalize_harness_path(
                "/tmp/workspace/tsz/TypeScript/tests/cases/compiler/foo.ts"
            ),
            "TypeScript/tests/cases/compiler/foo.ts",
        )

    def test_uses_last_typescript_segment(self):
        self.assertEqual(
            normalize_harness_path(
                "/tmp/TypeScript/workspace/tsz/TypeScript/tests/cases/compiler/foo.ts"
            ),
            "TypeScript/tests/cases/compiler/foo.ts",
        )


class TestParseRunnerOutput(unittest.TestCase):
    def test_pass_line(self):
        tmp = _write_tmp("PASS TypeScript/tests/cases/foo/bar.ts\n")
        try:
            tests = parse_runner_output(tmp)
        finally:
            os.unlink(tmp)
        self.assertIn("TypeScript/tests/cases/foo/bar.ts", tests)
        rec = tests["TypeScript/tests/cases/foo/bar.ts"]
        self.assertEqual(rec["status"], "PASS")
        self.assertEqual(rec["expected"], [])
        self.assertEqual(rec["actual"], [])

    def test_fail_with_codes(self):
        content = (
            "FAIL TypeScript/tests/cases/compiler/foo.ts\n"
            "  expected: [TS2322,TS2345]\n"
            "  actual: [TS2322]\n"
        )
        tmp = _write_tmp(content)
        try:
            tests = parse_runner_output(tmp)
        finally:
            os.unlink(tmp)
        rec = tests["TypeScript/tests/cases/compiler/foo.ts"]
        self.assertEqual(rec["status"], "FAIL")
        self.assertEqual(rec["expected"], ["TS2322", "TS2345"])
        self.assertEqual(rec["actual"], ["TS2322"])

    def test_fail_empty_codes(self):
        content = (
            "FAIL TypeScript/tests/cases/compiler/empty.ts\n"
            "  expected: []\n"
            "  actual: []\n"
        )
        tmp = _write_tmp(content)
        try:
            tests = parse_runner_output(tmp)
        finally:
            os.unlink(tmp)
        rec = tests["TypeScript/tests/cases/compiler/empty.ts"]
        self.assertEqual(rec["expected"], [])
        self.assertEqual(rec["actual"], [])

    def test_xfail_with_reason(self):
        content = "XFAIL TypeScript/tests/cases/compiler/known.ts (reason: pending)\n"
        tmp = _write_tmp(content)
        try:
            tests = parse_runner_output(tmp)
        finally:
            os.unlink(tmp)
        rec = tests["TypeScript/tests/cases/compiler/known.ts"]
        self.assertEqual(rec["status"], "XFAIL")
        self.assertEqual(rec["known_failure"], "reason: pending")

    def test_xfail_no_reason(self):
        content = (
            "XFAIL TypeScript/tests/cases/compiler/nofail.ts\n"
            "  expected: [TS2322]\n"
            "  actual: []\n"
        )
        tmp = _write_tmp(content)
        try:
            tests = parse_runner_output(tmp)
        finally:
            os.unlink(tmp)
        rec = tests["TypeScript/tests/cases/compiler/nofail.ts"]
        self.assertEqual(rec["status"], "XFAIL")
        self.assertEqual(rec["known_failure"], "")

    def test_skip_line(self):
        tmp = _write_tmp("SKIP TypeScript/tests/cases/skipped.ts\n")
        try:
            tests = parse_runner_output(tmp)
        finally:
            os.unlink(tmp)
        self.assertEqual(tests["TypeScript/tests/cases/skipped.ts"]["status"], "SKIP")

    def test_unsupported_line_preserves_reason(self):
        tmp = _write_tmp(
            "UNSUPPORTED TypeScript/tests/cases/unsupported.ts "
            "(typescript-7-unsupported-configuration)\n"
        )
        try:
            tests = parse_runner_output(tmp)
        finally:
            os.unlink(tmp)
        rec = tests["TypeScript/tests/cases/unsupported.ts"]
        self.assertEqual(rec["status"], "UNSUPPORTED")
        self.assertEqual(
            rec["unsupported_reason"],
            "typescript-7-unsupported-configuration",
        )

    def test_crash_line(self):
        tmp = _write_tmp("CRASH TypeScript/tests/cases/crashed.ts\n")
        try:
            tests = parse_runner_output(tmp)
        finally:
            os.unlink(tmp)
        self.assertEqual(tests["TypeScript/tests/cases/crashed.ts"]["status"], "CRASH")

    def test_timeout_line(self):
        tmp = _write_tmp("⏱️ TIMEOUT TypeScript/tests/cases/slow.ts\n")
        try:
            tests = parse_runner_output(tmp)
        finally:
            os.unlink(tmp)
        self.assertEqual(tests["TypeScript/tests/cases/slow.ts"]["status"], "TIMEOUT")

    def test_plain_timeout_line(self):
        tmp = _write_tmp("TIMEOUT TypeScript/tests/cases/plain-slow.ts\n")
        try:
            tests = parse_runner_output(tmp)
        finally:
            os.unlink(tmp)
        self.assertEqual(
            tests["TypeScript/tests/cases/plain-slow.ts"]["status"],
            "TIMEOUT",
        )

    def test_options_line(self):
        content = (
            "FAIL TypeScript/tests/cases/compiler/opts.ts\n"
            "  expected: [TS2322]\n"
            "  actual: []\n"
            "  options: --strict --target ES2015\n"
        )
        tmp = _write_tmp(content)
        try:
            tests = parse_runner_output(tmp)
        finally:
            os.unlink(tmp)
        rec = tests["TypeScript/tests/cases/compiler/opts.ts"]
        self.assertEqual(rec["options"], "--strict --target ES2015")

    def test_multiple_tests(self):
        content = (
            "PASS TypeScript/tests/cases/a.ts\n"
            "FAIL TypeScript/tests/cases/b.ts\n"
            "  expected: [TS2339]\n"
            "  actual: []\n"
            "PASS TypeScript/tests/cases/c.ts\n"
        )
        tmp = _write_tmp(content)
        try:
            tests = parse_runner_output(tmp)
        finally:
            os.unlink(tmp)
        self.assertEqual(len(tests), 3)
        self.assertEqual(tests["TypeScript/tests/cases/a.ts"]["status"], "PASS")
        self.assertEqual(tests["TypeScript/tests/cases/b.ts"]["status"], "FAIL")
        self.assertEqual(tests["TypeScript/tests/cases/b.ts"]["expected"], ["TS2339"])
        self.assertEqual(tests["TypeScript/tests/cases/c.ts"]["status"], "PASS")

    def test_fail_block_terminated_by_next_pass(self):
        content = (
            "FAIL TypeScript/tests/cases/x.ts\n"
            "  expected: [TS2322]\n"
            "  actual: [TS2322,TS2345]\n"
            "PASS TypeScript/tests/cases/y.ts\n"
        )
        tmp = _write_tmp(content)
        try:
            tests = parse_runner_output(tmp)
        finally:
            os.unlink(tmp)
        self.assertEqual(tests["TypeScript/tests/cases/x.ts"]["actual"], ["TS2322", "TS2345"])
        self.assertEqual(tests["TypeScript/tests/cases/y.ts"]["status"], "PASS")


class TestSummarizeRunnerOutput(unittest.TestCase):
    def test_partitions_candidates_and_runnable_rows(self):
        content = (
            "PASS pass.ts\n"
            "FAIL fail.ts\n"
            "  expected: [TS2322]\n"
            "  actual: []\n"
            "UNSUPPORTED unsupported.ts (typescript-7-unsupported-configuration)\n"
            "SKIP skipped.ts\n"
            "Candidates: 4\n"
            "Runnable: 2\n"
            "Unsupported: 1\n"
            "Skipped: 1\n"
            "Known failures: 0\n"
            "Crashed: 0\n"
            "Timeout: 0\n"
            "FINAL RESULTS: 1/2 passed (50.0%)\n"
        )
        tmp = _write_tmp(content)
        try:
            summary = summarize_runner_output(tmp)
            validated = require_complete_runner_summary(tmp)
        finally:
            os.unlink(tmp)

        self.assertEqual(summary["candidates"], 4)
        self.assertEqual(summary["runnable"], 2)
        self.assertEqual(summary["unsupported"], 1)
        self.assertEqual(summary["skipped"], 1)
        self.assertEqual(summary["recorded_candidates"], 4)
        self.assertEqual(summary["recorded_runnable"], 2)
        self.assertTrue(summary["partition_valid"])
        self.assertEqual(validated["passed"], 1)

    def test_legacy_output_derives_candidate_partition(self):
        content = (
            "PASS pass.ts\n"
            "SKIP skipped.ts\n"
            "Skipped: 1\n"
            "FINAL RESULTS: 1/1 passed (100.0%)\n"
        )
        tmp = _write_tmp(content)
        try:
            summary = summarize_runner_output(tmp)
        finally:
            os.unlink(tmp)

        self.assertEqual(summary["candidates"], 2)
        self.assertEqual(summary["runnable"], 1)
        self.assertEqual(summary["skipped"], 1)
        self.assertTrue(summary["partition_valid"])

    def test_complete_validator_rejects_duplicate_identity(self):
        content = (
            "PASS same.ts\nPASS same.ts\n"
            "Candidates: 2\nRunnable: 2\nUnsupported: 0\nSkipped: 0\n"
            "Known failures: 0\nCrashed: 0\nTimeout: 0\n"
            "FINAL RESULTS: 2/2 passed (100.0%)\n"
        )
        tmp = _write_tmp(content)
        try:
            with self.assertRaisesRegex(ValueError, "repeats terminal identities"):
                require_complete_runner_summary(tmp)
        finally:
            os.unlink(tmp)

    def test_complete_validator_rejects_status_arithmetic_mismatch(self):
        content = (
            "PASS pass.ts\nCRASH crash.ts\n"
            "Candidates: 2\nRunnable: 2\nUnsupported: 0\nSkipped: 0\n"
            "Known failures: 0\nCrashed: 0\nTimeout: 0\n"
            "FINAL RESULTS: 1/2 passed (50.0%)\n"
        )
        tmp = _write_tmp(content)
        try:
            with self.assertRaisesRegex(ValueError, "crashed identities"):
                require_complete_runner_summary(tmp)
        finally:
            os.unlink(tmp)

    def test_complete_validator_rejects_panic_and_signal_statuses_on_red_suite(self):
        content = (
            "FAIL fail.ts\n"
            "  expected: [TS2322]\n"
            "  actual: []\n"
            "Candidates: 1\nRunnable: 1\nUnsupported: 0\nSkipped: 0\n"
            "Known failures: 0\nCrashed: 0\nTimeout: 0\n"
            "FINAL RESULTS: 0/1 passed (0.0%)\n"
        )
        tmp = _write_tmp(content)
        try:
            self.assertEqual(
                require_complete_runner_summary(tmp, runner_status=1)["runner_status"],
                1,
            )
            for status in (101, 137):
                with self.assertRaisesRegex(ValueError, "exactly 1"):
                    require_complete_runner_summary(tmp, runner_status=status)
        finally:
            os.unlink(tmp)


class TestAnalyzeConformancePattern(unittest.TestCase):
    """Lock the pattern used by analyze-conformance.py to filter FAIL/XFAIL records."""

    def _parse_fail_xfail(self, content):
        tmp = _write_tmp(content)
        try:
            raw = parse_runner_output(tmp)
        finally:
            os.unlink(tmp)
        return [
            {**rec, "path": path}
            for path, rec in raw.items()
            if rec["status"] in ("FAIL", "XFAIL")
        ]

    def test_filters_out_non_failure_results(self):
        content = (
            "PASS TypeScript/tests/cases/a.ts\n"
            "SKIP TypeScript/tests/cases/b.ts\n"
            "UNSUPPORTED TypeScript/tests/cases/unsupported.ts "
            "(typescript-7-unsupported-configuration)\n"
            "FAIL TypeScript/tests/cases/c.ts\n"
            "  expected: [TS2322]\n"
            "  actual: []\n"
        )
        tests = self._parse_fail_xfail(content)
        self.assertEqual(len(tests), 1)
        self.assertEqual(tests[0]["path"], "TypeScript/tests/cases/c.ts")

    def test_includes_xfail(self):
        content = (
            "XFAIL TypeScript/tests/cases/known.ts\n"
            "  expected: [TS2322]\n"
            "  actual: []\n"
        )
        tests = self._parse_fail_xfail(content)
        self.assertEqual(len(tests), 1)
        self.assertEqual(tests[0]["status"], "XFAIL")

    def test_path_key_added_to_record(self):
        content = (
            "FAIL TypeScript/tests/cases/foo.ts\n"
            "  expected: [TS2345]\n"
            "  actual: [TS2339]\n"
        )
        tests = self._parse_fail_xfail(content)
        self.assertEqual(tests[0]["path"], "TypeScript/tests/cases/foo.ts")
        self.assertEqual(tests[0]["expected"], ["TS2345"])
        self.assertEqual(tests[0]["actual"], ["TS2339"])

    def test_wrong_code_diff_via_compute_diff(self):
        missing, extra = compute_diff(["TS2322", "TS2345"], ["TS2322", "TS2339"])
        self.assertEqual(missing, ["TS2345"])
        self.assertEqual(extra, ["TS2339"])

    def test_false_positive_diff_via_compute_diff(self):
        missing, extra = compute_diff([], ["TS7053", "TS7053"])
        self.assertEqual(missing, [])
        self.assertEqual(extra, ["TS7053", "TS7053"])

    def test_all_missing_diff_via_compute_diff(self):
        missing, extra = compute_diff(["TS2322", "TS2345"], [])
        self.assertEqual(missing, ["TS2322", "TS2345"])
        self.assertEqual(extra, [])


if __name__ == "__main__":
    unittest.main()
