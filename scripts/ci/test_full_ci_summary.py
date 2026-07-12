import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).with_name("full-ci-summary.py")
SPEC = importlib.util.spec_from_file_location("full_ci_summary", SCRIPT_PATH)
full_ci_summary = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = full_ci_summary
SPEC.loader.exec_module(full_ci_summary)


class ConformanceSummaryTests(unittest.TestCase):
    def test_reports_runnable_denominator_and_candidate_partition(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            metrics_dir = root / "metrics"
            logs_dir = root / "logs"
            metrics_dir.mkdir()
            logs_dir.mkdir()
            (metrics_dir / "conformance.json").write_text(
                json.dumps(
                    {
                        "passed": 8,
                        "total": 8,
                        "runnable": 8,
                        "candidates": 10,
                        "unsupported": 1,
                        "skipped": 1,
                        "workers": 2,
                    }
                ),
                encoding="utf-8",
            )
            lines = []
            full_ci_summary.conformance_summary(metrics_dir, logs_dir, lines, 0)

        rendered = "\n".join(lines)
        self.assertIn("Passed `8` of `8` runnable tests", rendered)
        self.assertIn(
            "Candidate domain: `10` total; `1` unsupported; `1` skipped",
            rendered,
        )


if __name__ == "__main__":
    unittest.main()
