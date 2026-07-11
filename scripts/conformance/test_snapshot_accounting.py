import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("conformance.sh")


class SnapshotAccountingContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.script = SCRIPT.read_text(encoding="utf-8")

    def test_snapshot_completeness_checks_candidates_and_runnable_rows(self):
        self.assertIn("summarize_runner_output", self.script)
        self.assertIn('d.get(\'recorded_candidates\', 0)', self.script)
        self.assertIn('d.get(\'recorded_runnable\', 0)', self.script)
        self.assertIn(
            '[ "$recorded_results" -ne "$candidate_tests" ]',
            self.script,
        )
        self.assertIn(
            '[ "$recorded_runnable" -ne "$total_tests" ]',
            self.script,
        )
        self.assertIn(
            "Incomplete candidate coverage cannot be saved, including with --force",
            self.script,
        )

    def test_snapshot_summary_persists_full_partition(self):
        for field in (
            "'candidates': candidates",
            "'total_tests': runnable",
            "'runnable': runnable",
            "'unsupported': unsupported",
            "'skipped': skipped",
        ):
            with self.subTest(field=field):
                self.assertIn(field, self.script)
        self.assertIn(
            "candidates != runnable + unsupported + skipped",
            self.script,
        )

    def test_detail_generation_is_required(self):
        detail_call = self.script.index("build-snapshot-detail.py")
        analyze_call = self.script.index("analyze-conformance.py", detail_call)
        block = self.script[detail_call:analyze_call]
        self.assertIn("failed to build conformance detail snapshot", block)
        self.assertNotIn("|| true", block)


if __name__ == "__main__":
    unittest.main()
