"""Behavior-lock unit tests for scripts/lib/conformance_query.py."""

import json
import sys
import tempfile
import unittest
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))
from lib.conformance_query import (
    basename,
    code_counts,
    is_fingerprint_only,
    is_same_code_count_drift,
    load_detail,
)


class TestBasename(unittest.TestCase):
    def test_returns_last_segment(self):
        self.assertEqual(
            basename("TypeScript/tests/cases/compiler/foo.ts"), "foo.ts"
        )

    def test_returns_path_when_no_slash(self):
        self.assertEqual(basename("foo.ts"), "foo.ts")

    def test_handles_empty_string(self):
        self.assertEqual(basename(""), "")

    def test_handles_trailing_slash(self):
        # Trailing slash yields empty final segment, matching the
        # `path.rsplit("/", 1)[-1]` behavior the helper preserves.
        self.assertEqual(basename("foo/bar/"), "")

    def test_preserves_dotfiles(self):
        self.assertEqual(basename("a/b/.hidden"), ".hidden")


class TestCodeCounts(unittest.TestCase):
    def test_counts_each_code(self):
        counts = code_counts(["TS2322", "TS2322", "TS2345"])
        self.assertEqual(counts["TS2322"], 2)
        self.assertEqual(counts["TS2345"], 1)

    def test_empty_iterable(self):
        self.assertEqual(code_counts([]), Counter())

    def test_returns_counter_type(self):
        self.assertIsInstance(code_counts(["TS1"]), Counter)


class TestIsFingerprintOnly(unittest.TestCase):
    def test_true_when_codes_match(self):
        failure = {"e": ["TS2322", "TS2345"], "a": ["TS2345", "TS2322"]}
        self.assertTrue(is_fingerprint_only(failure))

    def test_true_with_duplicates(self):
        failure = {"e": ["TS2322", "TS2322"], "a": ["TS2322", "TS2322"]}
        self.assertTrue(is_fingerprint_only(failure))

    def test_false_when_codes_differ(self):
        failure = {"e": ["TS2322"], "a": ["TS2345"]}
        self.assertFalse(is_fingerprint_only(failure))

    def test_false_when_count_differs(self):
        failure = {"e": ["TS2322"], "a": ["TS2322", "TS2322"]}
        self.assertFalse(is_fingerprint_only(failure))

    def test_false_when_expected_empty(self):
        failure = {"e": [], "a": ["TS2322"]}
        self.assertFalse(is_fingerprint_only(failure))

    def test_false_when_actual_empty(self):
        failure = {"e": ["TS2322"], "a": []}
        self.assertFalse(is_fingerprint_only(failure))

    def test_false_when_both_empty(self):
        # Both empty is degenerate (would imply no failure to classify) and
        # must not be reported as fingerprint-only.
        self.assertFalse(is_fingerprint_only({"e": [], "a": []}))

    def test_handles_missing_keys(self):
        # The compact format omits empty lists; the helper must treat that
        # as an empty list, not a KeyError.
        self.assertFalse(is_fingerprint_only({}))


class TestIsSameCodeCountDrift(unittest.TestCase):
    def test_true_when_counts_differ_but_set_matches(self):
        failure = {"e": ["TS2322", "TS2322"], "a": ["TS2322"]}
        self.assertTrue(is_same_code_count_drift(failure))

    def test_false_when_counts_match(self):
        failure = {"e": ["TS2322", "TS2322"], "a": ["TS2322", "TS2322"]}
        self.assertFalse(is_same_code_count_drift(failure))

    def test_false_when_code_sets_differ(self):
        failure = {"e": ["TS2322"], "a": ["TS2345"]}
        self.assertFalse(is_same_code_count_drift(failure))

    def test_false_when_expected_empty(self):
        self.assertFalse(is_same_code_count_drift({"e": [], "a": ["TS2322"]}))

    def test_false_when_actual_empty(self):
        self.assertFalse(is_same_code_count_drift({"e": ["TS2322"], "a": []}))


class TestLoadDetail(unittest.TestCase):
    def test_loads_valid_snapshot(self):
        data = {"summary": {"total": 1, "passed": 1, "failed": 0}, "failures": {}}
        with tempfile.NamedTemporaryFile(
            mode="w", suffix=".json", delete=False
        ) as f:
            json.dump(data, f)
            path = Path(f.name)
        try:
            result = load_detail(path)
            self.assertEqual(result["summary"]["passed"], 1)
            self.assertEqual(result["failures"], {})
        finally:
            path.unlink()

    def test_exits_when_file_missing(self):
        missing = Path("/tmp/does_not_exist_tsz_conformance_test.json")
        with self.assertRaises(SystemExit):
            load_detail(missing)


if __name__ == "__main__":
    unittest.main()
