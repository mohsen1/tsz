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
from lib.results import parse_runner_output, compute_diff


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

    def test_filters_out_pass_and_skip(self):
        content = (
            "PASS TypeScript/tests/cases/a.ts\n"
            "SKIP TypeScript/tests/cases/b.ts\n"
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
