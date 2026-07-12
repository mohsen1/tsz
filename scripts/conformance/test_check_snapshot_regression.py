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


def snapshot(
    passed,
    failures,
    categories=None,
    total=100,
    candidates=None,
    unsupported=0,
    skipped=0,
):
    return ConformanceSnapshot(
        passed=passed,
        total=total,
        candidates=(
            candidates if candidates is not None else total + unsupported + skipped
        ),
        unsupported=unsupported,
        skipped=skipped,
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

    def test_allows_new_failures_when_fixed_failures_outnumber_them(self):
        comparison = compare_snapshots(
            snapshot(
                98,
                {
                    "old-1.ts": {"e": ["TS1"]},
                    "old-2.ts": {"e": ["TS2"]},
                },
            ),
            snapshot(99, {"new.ts": {"e": ["TS3"]}}),
        )

        self.assertEqual(comparison.fixed_failures, ["old-1.ts", "old-2.ts"])
        self.assertEqual(comparison.new_failures, ["new.ts"])
        self.assertFalse(comparison.has_blocking_regression())

    def test_blocks_new_failures_when_the_failure_set_gets_worse(self):
        comparison = compare_snapshots(
            snapshot(98, {"old.ts": {"e": ["TS1"]}}),
            snapshot(
                98,
                {
                    "new-1.ts": {"e": ["TS2"]},
                    "new-2.ts": {"e": ["TS3"]},
                },
            ),
        )

        self.assertEqual(comparison.fixed_failures, ["old.ts"])
        self.assertEqual(comparison.new_failures, ["new-1.ts", "new-2.ts"])
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

    def test_normalizes_absolute_and_repo_relative_failure_keys(self):
        comparison = compare_snapshots(
            snapshot(
                98,
                {
                    "/Users/mohsen/code/tsz/TypeScript/tests/cases/compiler/same.ts": {
                        "e": ["TS1"],
                        "a": ["TS1"],
                    }
                },
            ),
            snapshot(
                98,
                {
                    "TypeScript/tests/cases/compiler/same.ts": {
                        "e": ["TS1"],
                        "a": ["TS1"],
                    }
                },
            ),
        )

        self.assertEqual(comparison.fixed_failures, [])
        self.assertEqual(comparison.new_failures, [])
        self.assertEqual(comparison.changed_failures, [])

    def test_computes_category_delta(self):
        comparison = compare_snapshots(
            snapshot(98, {}, {"wrong_code": 7, "fingerprint_only": 4}),
            snapshot(99, {}, {"wrong_code": 5, "fingerprint_only": 8}),
        )

        self.assertEqual(comparison.category_delta["wrong_code"], -2)
        self.assertEqual(comparison.category_delta["fingerprint_only"], 4)

    def test_unsupported_candidates_are_outside_runnable_denominator(self):
        comparison = compare_snapshots(
            snapshot(1, {}, total=2, candidates=2),
            snapshot(1, {}, total=1, candidates=2, unsupported=1),
        )

        self.assertEqual(comparison.head.runnable, 1)
        self.assertEqual(comparison.head.candidates, 2)
        self.assertEqual(comparison.head.unsupported, 1)
        self.assertEqual(comparison.pass_delta, 0)
        self.assertFalse(comparison.has_blocking_regression())

    def test_new_summary_accounting_prefers_explicit_runnable(self):
        accounting = check_snapshot_regression._summary_accounting(
            {
                "summary": {
                    "candidates": 12,
                    "runnable": 9,
                    "total_tests": 12,
                    "unsupported": 2,
                    "skipped": 1,
                }
            },
            {},
        )

        self.assertEqual(accounting, (12, 9, 2, 1))

    def test_legacy_summary_uses_detail_total_as_candidate_count(self):
        accounting = check_snapshot_regression._summary_accounting(
            {"summary": {"total_tests": 9}},
            {"summary": {"total": 10, "skipped": 1}},
        )

        self.assertEqual(accounting, (10, 9, 0, 1))


if __name__ == "__main__":
    unittest.main()
