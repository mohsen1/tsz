#!/usr/bin/env python3
"""Contract tests for the strict clean-slate nextest wrapper."""

from __future__ import annotations

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import textwrap
import unittest


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "ci" / "unit-nextest.sh"


class UnitNextestContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp_dir = Path(tempfile.mkdtemp(prefix="rewrite-unit-nextest-"))
        self.addCleanup(shutil.rmtree, self.temp_dir)
        script_dir = self.temp_dir / "scripts" / "ci"
        script_dir.mkdir(parents=True)
        self.script = script_dir / "unit-nextest.sh"
        shutil.copy2(SCRIPT, self.script)

        self.stub_dir = self.temp_dir / "stub-bin"
        self.stub_dir.mkdir()
        self.calls = self.temp_dir / "cargo.calls"
        cargo = self.stub_dir / "cargo"
        cargo.write_text(
            textwrap.dedent(
                """\
                #!/usr/bin/env bash
                printf '%s\\n' "$*" >> "$CARGO_CALLS"
                rc="${NEXT_RC:-0}"
                if [[ "$1" == "nextest" && "$rc" != "4" && "${NO_JUNIT:-0}" != "1" ]]; then
                  mkdir -p "$REWRITE_TEST_ROOT/target/nextest/signoff"
                  printf '<testsuites/>\\n' > "$REWRITE_TEST_ROOT/target/nextest/signoff/junit.xml"
                fi
                exit "$rc"
                """
            ),
            encoding="utf-8",
        )
        cargo.chmod(0o755)

    def run_script(self, *args: str, **env_overrides: str) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        env.update(
            {
                "PATH": f"{self.stub_dir}:{env['PATH']}",
                "CARGO_CALLS": str(self.calls),
                "REWRITE_TEST_ROOT": str(self.temp_dir),
            }
        )
        env.update(env_overrides)
        return subprocess.run(
            ["bash", str(self.script), *args],
            cwd=self.temp_dir,
            env=env,
            text=True,
            capture_output=True,
            check=False,
        )

    def cargo_call(self) -> str:
        return self.calls.read_text(encoding="utf-8").strip()

    def test_explicit_active_packages_form_one_strict_pass(self) -> None:
        out = self.temp_dir / "junit"
        result = self.run_script(
            "--junit-dir",
            str(out),
            "--packages",
            "tsz-core tsz-cli tsz-conformance",
            UNIT_NEXTEST_TEST_THREADS="3",
            CARGO_BUILD_JOBS="2",
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        call = self.cargo_call()
        self.assertIn("nextest run --profile signoff --cargo-profile ci-unit", call)
        self.assertIn("-p tsz-core -p tsz-cli -p tsz-conformance", call)
        self.assertIn("--build-jobs 2", call)
        self.assertIn("--test-threads 3", call)
        self.assertTrue((out / "rewrite.xml").is_file())

    def test_workspace_selection_is_strict(self) -> None:
        result = self.run_script(
            "--junit-dir",
            str(self.temp_dir / "junit"),
            "--workspace",
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("--workspace", self.cargo_call())

    def test_rejects_non_workspace_package(self) -> None:
        result = self.run_script(
            "--junit-dir",
            str(self.temp_dir / "junit"),
            "--packages",
            "tsz-core retired-compiler",
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("not in the active rewrite workspace", result.stderr)

    def test_rejects_ambiguous_selection(self) -> None:
        result = self.run_script(
            "--junit-dir",
            str(self.temp_dir / "junit"),
            "--workspace",
            "--packages",
            "tsz-core",
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("mutually exclusive", result.stderr)

    def test_test_failure_is_not_baselined(self) -> None:
        result = self.run_script(
            "--junit-dir",
            str(self.temp_dir / "junit"),
            "--packages",
            "tsz-core",
            NEXT_RC="100",
        )
        self.assertEqual(result.returncode, 100)
        self.assertIn("active rewrite tests failed", result.stderr)

    def test_empty_narrow_selection_requires_explicit_allowance(self) -> None:
        denied = self.run_script(
            "--junit-dir",
            str(self.temp_dir / "denied"),
            "--packages",
            "tsz-core",
            NEXT_RC="4",
        )
        self.assertEqual(denied.returncode, 4)
        allowed = self.run_script(
            "--junit-dir",
            str(self.temp_dir / "allowed"),
            "--packages",
            "tsz-core",
            "--allow-no-reports",
            NEXT_RC="4",
        )
        self.assertEqual(allowed.returncode, 0, allowed.stderr)

    def test_missing_junit_is_an_infrastructure_failure(self) -> None:
        result = self.run_script(
            "--junit-dir",
            str(self.temp_dir / "junit"),
            "--packages",
            "tsz-core",
            NO_JUNIT="1",
        )
        self.assertEqual(result.returncode, 3)
        self.assertIn("did not produce", result.stderr)


if __name__ == "__main__":
    unittest.main()
