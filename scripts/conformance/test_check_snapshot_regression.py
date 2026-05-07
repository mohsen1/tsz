import importlib.util
import sys
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).with_name("check-snapshot-regression.py")
SPEC = importlib.util.spec_from_file_location("check_snapshot_regression", SCRIPT_PATH)
check_snapshot_regression = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = check_snapshot_regression
SPEC.loader.exec_module(check_snapshot_regression)


ConformanceSnapshot = check_snapshot_regression.ConformanceSnapshot
compare_snapshots = check_snapshot_regression.compare_snapshots


def snapshot(passed, failures, categories=None, total=100):
    return ConformanceSnapshot(
        passed=passed,
        total=total,
        failures=failures,
        categories=categories or {},
    )


class CheckSnapshotRegressionTests(unittest.TestCase):
    def test_blocks_lower_pass_count(self):
        comparison = compare_snapshots(
            snapshot(99, {}),
            snapshot(98, {}),
        )

        self.assertEqual(comparison.pass_delta, -1)
        self.assertTrue(comparison.has_blocking_regression())

    def test_blocks_new_failure_even_when_net_positive(self):
        comparison = compare_snapshots(
            snapshot(98, {"old.ts": {"e": ["TS1"]}}),
            snapshot(99, {"new.ts": {"e": ["TS2"]}}),
        )

        self.assertEqual(comparison.fixed_failures, ["old.ts"])
        self.assertEqual(comparison.new_failures, ["new.ts"])
        self.assertTrue(comparison.has_blocking_regression())

    def test_allows_explicit_new_failure_override_when_pass_count_does_not_drop(self):
        comparison = compare_snapshots(
            snapshot(98, {"old.ts": {"e": ["TS1"]}}),
            snapshot(99, {"new.ts": {"e": ["TS2"]}}),
        )

        self.assertFalse(comparison.has_blocking_regression(allow_new_failures=True))

    def test_reports_changed_still_failing_tests(self):
        comparison = compare_snapshots(
            snapshot(98, {"same.ts": {"e": ["TS1"], "a": ["TS1"]}}),
            snapshot(98, {"same.ts": {"e": ["TS1"], "a": ["TS2"]}}),
        )

        self.assertEqual(comparison.changed_failures, ["same.ts"])

    def test_computes_category_delta(self):
        comparison = compare_snapshots(
            snapshot(98, {}, {"wrong_code": 7, "fingerprint_only": 4}),
            snapshot(99, {}, {"wrong_code": 5, "fingerprint_only": 8}),
        )

        self.assertEqual(comparison.category_delta["wrong_code"], -2)
        self.assertEqual(comparison.category_delta["fingerprint_only"], 4)


if __name__ == "__main__":
    unittest.main()
