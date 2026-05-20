"""Behavior-lock unit tests for scripts/lib/query_snapshot.py."""

import io
import json
import sys
import tempfile
import unittest
from collections import Counter
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).parent.parent))
from lib.query_snapshot import (
    filter_by_name,
    load_snapshot,
    print_top_counter,
    print_truncated_more,
)


class TestLoadSnapshot(unittest.TestCase):
    def test_loads_valid_json(self):
        data = {"summary": {"passed": 10}, "results": []}
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
            json.dump(data, f)
            path = Path(f.name)
        try:
            result = load_snapshot(path)
            self.assertEqual(result["summary"]["passed"], 10)
            self.assertEqual(result["results"], [])
        finally:
            path.unlink()

    def test_exits_when_file_missing(self):
        missing = Path("/tmp/does_not_exist_tsz_test.json")
        with self.assertRaises(SystemExit):
            load_snapshot(missing, "some hint")

    def test_prints_path_and_hint_on_missing(self):
        missing = Path("/tmp/does_not_exist_tsz_test.json")
        with patch("builtins.print") as mock_print, self.assertRaises(SystemExit):
            load_snapshot(missing, "run --json-out to regenerate")
        printed = " ".join(str(c) for c in mock_print.call_args_list)
        self.assertIn("does_not_exist_tsz_test.json", printed)
        self.assertIn("run --json-out", printed)


class TestPrintTopCounter(unittest.TestCase):
    def _capture(self, counter, top):
        buf = io.StringIO()
        orig, sys.stdout = sys.stdout, buf
        try:
            print_top_counter(counter, top)
        finally:
            sys.stdout = orig
        return buf.getvalue()

    def test_shows_top_n_entries(self):
        c = Counter({"alpha": 100, "beta": 50, "gamma": 10, "delta": 1})
        out = self._capture(c, 2)
        self.assertIn("alpha", out)
        self.assertIn("beta", out)
        self.assertNotIn("gamma", out)
        self.assertNotIn("delta", out)

    def test_count_is_right_aligned(self):
        c = Counter({"msg": 5})
        out = self._capture(c, 10)
        # Count must appear with right-alignment padding before it
        self.assertIn("   5  msg", out)

    def test_empty_counter_produces_no_output(self):
        out = self._capture(Counter(), 10)
        self.assertEqual(out, "")

    def test_top_zero_produces_no_output(self):
        c = Counter({"x": 1})
        out = self._capture(c, 0)
        self.assertEqual(out, "")

    def test_shows_all_when_top_exceeds_size(self):
        c = Counter({"a": 3, "b": 2})
        out = self._capture(c, 100)
        self.assertIn("a", out)
        self.assertIn("b", out)


class TestFilterByName(unittest.TestCase):
    ITEMS = [
        {"name": "TestFoo"},
        {"name": "TestBar"},
        {"name": "BazTest"},
        {"name": "unrelated"},
    ]

    def test_case_insensitive_match(self):
        result = filter_by_name(self.ITEMS, "FOO")
        self.assertEqual([r["name"] for r in result], ["TestFoo"])

    def test_substring_match(self):
        result = filter_by_name(self.ITEMS, "test")
        names = [r["name"] for r in result]
        self.assertIn("TestFoo", names)
        self.assertIn("TestBar", names)
        self.assertIn("BazTest", names)
        self.assertNotIn("unrelated", names)

    def test_no_match_returns_empty(self):
        result = filter_by_name(self.ITEMS, "zzz")
        self.assertEqual(result, [])

    def test_custom_name_key(self):
        items = [{"file": "foo.ts"}, {"file": "bar.ts"}]
        result = filter_by_name(items, "foo", name_key="file")
        self.assertEqual(len(result), 1)
        self.assertEqual(result[0]["file"], "foo.ts")

    def test_missing_key_does_not_crash(self):
        items = [{"other": "foo"}, {"name": "foo"}]
        result = filter_by_name(items, "foo")
        self.assertEqual(len(result), 1)


class TestPrintTruncatedMore(unittest.TestCase):
    def _capture(self, items, top, **kwargs):
        buf = io.StringIO()
        orig, sys.stdout = sys.stdout, buf
        try:
            print_truncated_more(items, top, **kwargs)
        finally:
            sys.stdout = orig
        return buf.getvalue()

    def test_prints_tail_when_items_exceed_top(self):
        items = list(range(100))
        out = self._capture(items, 40)
        self.assertEqual(out, "  ... and 60 more\n")

    def test_no_output_when_items_equal_top(self):
        items = list(range(40))
        out = self._capture(items, 40)
        self.assertEqual(out, "")

    def test_no_output_when_items_below_top(self):
        items = list(range(10))
        out = self._capture(items, 40)
        self.assertEqual(out, "")

    def test_no_output_when_items_empty(self):
        out = self._capture([], 40)
        self.assertEqual(out, "")

    def test_custom_indent(self):
        items = list(range(50))
        out = self._capture(items, 30, indent="     ")
        self.assertEqual(out, "     ... and 20 more\n")

    def test_zero_indent(self):
        items = list(range(5))
        out = self._capture(items, 2, indent="")
        self.assertEqual(out, "... and 3 more\n")

    def test_off_by_one_just_over(self):
        items = list(range(41))
        out = self._capture(items, 40)
        self.assertEqual(out, "  ... and 1 more\n")

    def test_works_with_tuples(self):
        items = tuple(range(50))
        out = self._capture(items, 30)
        self.assertEqual(out, "  ... and 20 more\n")


if __name__ == "__main__":
    unittest.main()
