import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("extract-baseline.py")


class ExtractBaselineTruthTests(unittest.TestCase):
    def run_extract(self, text):
        with tempfile.NamedTemporaryFile("w", encoding="utf-8") as source:
            source.write(text)
            source.flush()
            return subprocess.run(
                [sys.executable, str(SCRIPT), source.name],
                text=True,
                capture_output=True,
                check=False,
            )

    def test_preserves_crash_and_timeout_identities(self):
        result = self.run_extract(
            "CRASH crash.ts\n"
            "TIMEOUT slow.ts (exceeded 90s)\n"
            "Candidates: 2\nRunnable: 2\nUnsupported: 0\nSkipped: 0\n"
            "Known failures: 0\nCrashed: 1\nTimeout: 1\n"
            "FINAL RESULTS: 0/2 passed (0.0%)\n"
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "CRASH crash.ts\nTIMEOUT slow.ts\n")

    def test_refuses_incomplete_runner_output(self):
        result = self.run_extract("PASS only.ts\n")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("refusing baseline extraction", result.stderr)

    def test_refuses_duplicate_terminal_identity(self):
        result = self.run_extract(
            "PASS same.ts\nPASS same.ts\n"
            "FINAL RESULTS: 2/2 passed (100.0%)\n"
            "Candidates: 2\nRunnable: 2\nUnsupported: 0\nSkipped: 0\n"
            "Known failures: 0\nCrashed: 0\nTimeout: 0\n"
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("repeats terminal identities", result.stderr)


if __name__ == "__main__":
    unittest.main()
