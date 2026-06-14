"""Unit tests for check-clippy-warn-ratchet.py (#13453)."""

import importlib.util
import sys
import unittest
from pathlib import Path

_SCRIPT = Path(__file__).with_name("check-clippy-warn-ratchet.py")
_SPEC = importlib.util.spec_from_file_location("check_clippy_warn_ratchet", _SCRIPT)
_mod = importlib.util.module_from_spec(_SPEC)  # type: ignore[arg-type]
sys.modules[_SPEC.name] = _mod  # type: ignore[union-attr]
_SPEC.loader.exec_module(_mod)  # type: ignore[union-attr]

check_ratchet = _mod.check_ratchet


class CheckRatchetTests(unittest.TestCase):
    def test_empty_live_empty_baseline_ok(self):
        self.assertEqual(check_ratchet({}, {}), [])

    def test_live_at_baseline_ok(self):
        self.assertEqual(
            check_ratchet({"clippy::foo": 5}, {"clippy::foo": 5}),
            [],
        )

    def test_live_below_baseline_ok(self):
        self.assertEqual(
            check_ratchet({"clippy::foo": 3}, {"clippy::foo": 5}),
            [],
        )

    def test_live_above_baseline_is_regression(self):
        result = check_ratchet({"clippy::foo": 6}, {"clippy::foo": 5})
        self.assertEqual(len(result), 1)
        self.assertIn("clippy::foo", result[0])
        self.assertIn("+1", result[0])

    def test_new_lint_above_zero_is_regression(self):
        result = check_ratchet({"clippy::bar": 2}, {})
        self.assertEqual(len(result), 1)
        self.assertIn("clippy::bar", result[0])

    def test_new_lint_zero_implicit_baseline_ok(self):
        self.assertEqual(check_ratchet({}, {"clippy::bar": 3}), [])

    def test_multiple_regressions_all_reported(self):
        live = {"clippy::a": 3, "clippy::b": 7}
        baseline = {"clippy::a": 2, "clippy::b": 5}
        result = check_ratchet(live, baseline)
        self.assertEqual(len(result), 2)
        joined = "\n".join(result)
        self.assertIn("clippy::a", joined)
        self.assertIn("clippy::b", joined)

    def test_mixed_regression_and_improvement(self):
        live = {"clippy::improved": 1, "clippy::regressed": 4}
        baseline = {"clippy::improved": 5, "clippy::regressed": 2}
        result = check_ratchet(live, baseline)
        self.assertEqual(len(result), 1)
        self.assertIn("clippy::regressed", result[0])

    def test_result_is_sorted(self):
        live = {"clippy::z": 2, "clippy::a": 2}
        baseline = {}
        result = check_ratchet(live, baseline)
        self.assertEqual(len(result), 2)
        self.assertLess(result[0], result[1])


if __name__ == "__main__":
    unittest.main()
