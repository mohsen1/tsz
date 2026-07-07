"""Contract tests for unit-test rc aggregation in full-ci.sh.

Regression coverage for issue #15404: `run_unit_tests` and
`run_checker_integration_tests` run several `nextest` batches with `set -e`
disabled (via `timed`). Each batch must contribute to the returned status;
otherwise a failing non-final batch is masked by a green final batch and a red
unit run reports rc=0.

These tests extract the two shell functions verbatim, stub `cargo` so a chosen
batch fails, and assert the aggregated exit status.
"""

import pathlib
import subprocess
import textwrap
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
FULL_CI = ROOT / "scripts" / "ci" / "full-ci.sh"


class UnitRcAggregationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        script = FULL_CI.read_text(encoding="utf-8")
        cls.run_unit_tests = cls._function_body(
            script, "run_unit_tests", "\nrun_checker_integration_tests() {"
        )
        cls.run_checker = cls._function_body(
            script, "run_checker_integration_tests", "\nbuild_unit_test_archive() {"
        )

    @staticmethod
    def _function_body(script, name, end_marker):
        start = script.index(f"{name}() {{")
        end = script.index(end_marker, start)
        return script[start:end]

    def _run(self, cargo_stub, env_line="", packages="tsz-core tsz-checker"):
        # Mirror the production caller: `timed()` disables `set -e` around the
        # invocation, so a failing batch does not abort the run — the function
        # is responsible for returning the aggregated rc via `|| rc=...`. We
        # reproduce that `set +e; run; rc=$?; set -e` frame here.
        harness = textwrap.dedent(
            f"""#!/usr/bin/env bash
            set -Eeuo pipefail
            {env_line}
            ci_section() {{ :; }}
            CARGO_BUILD_JOBS=1
            UNIT_NEXTEST_TEST_THREADS=1
            TSZ_CI_CHECKER_TEST_BATCH_SIZE=2
            unit_test_packages() {{ printf '%s\\n' {packages}; }}
            checker_integration_test_names() {{ printf '%s\\n' a b c d e; }}
            {cargo_stub}
            {self.run_checker}
            {self.run_unit_tests}
            set +e
            run_unit_tests
            rc=$?
            set -e
            echo "RC=$rc"
            """
        )
        proc = subprocess.run(["bash", "-c", harness], capture_output=True, text=True)
        # The harness itself must exit 0 (it always ends on `echo`); the tested
        # rc is reported via the RC= line.
        self.assertEqual(
            proc.returncode, 0, msg=f"harness aborted:\n{proc.stdout}\n{proc.stderr}"
        )
        rc_lines = [ln for ln in proc.stdout.splitlines() if ln.startswith("RC=")]
        self.assertEqual(len(rc_lines), 1, msg=f"missing RC line:\n{proc.stdout}")
        return int(rc_lines[0][len("RC=") :])

    # A cargo stub that fails whenever `token` appears in its argument list.
    @staticmethod
    def _fail_on(token):
        return f'cargo() {{ for a in "$@"; do [ "$a" = "{token}" ] && return 100; done; return 0; }}'

    def test_all_green_returns_zero(self):
        self.assertEqual(self._run("cargo() { return 0; }"), 0)

    def test_nonfinal_checker_batch_failure_propagates(self):
        # Batches of 2 over (a b c d e): first batch (a b) fails, final batch
        # (e) is green. The old code returned the final batch's 0.
        self.assertNotEqual(self._run(self._fail_on("b")), 0)

    def test_middle_checker_batch_failure_propagates(self):
        self.assertNotEqual(self._run(self._fail_on("d")), 0)

    def test_general_package_failure_propagates(self):
        # The non-checker batch fails; every checker batch is green.
        self.assertNotEqual(self._run(self._fail_on("tsz-core")), 0)

    def test_skip_checker_integration_still_reports_general_failure(self):
        rc = self._run(
            self._fail_on("tsz-core"),
            env_line="export TSZ_CI_UNIT_SKIP_CHECKER_INTEGRATION=1",
        )
        self.assertNotEqual(rc, 0)


if __name__ == "__main__":
    unittest.main()
