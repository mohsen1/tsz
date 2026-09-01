#!/usr/bin/env python3
"""Regression witnesses for seed-oracle's raw oracle stream boundary."""

from __future__ import annotations

import json
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SEED_ORACLE = ROOT / "scripts/reset/seed-oracle.py"
DIAGNOSTIC = "case.ts(1,1): error TS2322: Type 'number' is not assignable to type 'string'."


class SeedOracleStreamContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(prefix="tsz-seed-stream-")
        self.repo = Path(self.temp.name)
        (self.repo / "scripts/conformance").mkdir(parents=True)
        (self.repo / "tests/rewrite-seed/cases").mkdir(parents=True)
        (self.repo / "scripts/conformance/typescript-versions.json").write_text(
            json.dumps(
                {
                    "current": "ts7",
                    "mappings": {"ts7": {"npm": "7.0.2"}},
                }
            ),
            encoding="utf-8",
        )
        (self.repo / "tests/rewrite-seed/matrix.json").write_text(
            json.dumps(
                {
                    "pinned_typescript": "7.0.2",
                    "common_flags": [],
                    "rust_span_length_test": "stream-contract",
                    "cases": [
                        {
                            "name": "diagnostic-stream",
                            "mode": "diagnostics",
                            "source": "cases/input.ts",
                        }
                    ],
                }
            ),
            encoding="utf-8",
        )
        (self.repo / "tests/rewrite-seed/cases/input.ts").write_text(
            "const value: string = 1;\n", encoding="utf-8"
        )
        self.oracle = self.repo / "scripts/conformance/oracle.sh"
        self.candidate = self.repo / "candidate-tsz"
        self._write_executable(
            self.candidate,
            f"printf '%s\\n' {json.dumps(DIAGNOSTIC)}\nexit 1\n",
        )

    def tearDown(self) -> None:
        self.temp.cleanup()

    @staticmethod
    def _write_executable(path: Path, body: str) -> None:
        path.write_text("#!/usr/bin/env bash\nset -euo pipefail\n" + body, encoding="utf-8")
        path.chmod(0o755)

    def _run(self) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                "python3",
                str(SEED_ORACLE),
                "--repo-root",
                str(self.repo),
                "--tsz",
                str(self.candidate),
            ],
            capture_output=True,
            text=True,
            encoding="utf-8",
            check=False,
        )

    def test_oracle_metadata_on_stderr_does_not_change_seed_result(self) -> None:
        self._write_executable(
            self.oracle,
            "printf '%s\\n' 'verified TypeScript 7.0.2' >&2\n"
            f"printf '%s\\n' {json.dumps(DIAGNOSTIC)}\n"
            "exit 1\n",
        )

        result = self._run()

        self.assertEqual(0, result.returncode, result.stderr)
        self.assertIn("PASS diagnostic-stream", result.stdout)
        self.assertIn("PASS 1 seed case(s)", result.stdout)

    def test_oracle_metadata_on_stdout_fails_closed(self) -> None:
        self._write_executable(
            self.oracle,
            "printf '%s\\n' 'verified TypeScript 7.0.2'\n"
            f"printf '%s\\n' {json.dumps(DIAGNOSTIC)}\n"
            "exit 1\n",
        )

        result = self._run()

        self.assertEqual(1, result.returncode)
        self.assertIn("oracle produced unparsed stdout", result.stderr)
        self.assertIn("verified TypeScript 7.0.2", result.stderr)


if __name__ == "__main__":
    unittest.main()
