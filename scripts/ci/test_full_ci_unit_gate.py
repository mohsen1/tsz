"""Contract tests for the unit-lane known-failures gate wiring in full-ci.sh.

Successor to the #15404 rc-aggregation tests (test_full_ci_unit_rc.py, removed
with the #15647 re-mask): since #15646 the unit lane runs
scripts/ci/unit-nextest.sh (which records one junit per pass and exits nonzero
only for infrastructure failures) and gates on
scripts/ci/known-failures-check.mjs. These tests extract the wiring functions
verbatim from full-ci.sh, stub the runner and `node`, and assert:

* an infrastructure failure fails the job and never reaches the gate;
* the gate's verdict is the job's verdict when tests ran;
* a run that selected no tests skips the gate instead of failing on a missing
  junit;
* narrow-override validation errors propagate;
* the checker-integration suite shares the same wiring.
"""

import os
import pathlib
import subprocess
import tempfile
import textwrap
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
FULL_CI = ROOT / "scripts" / "ci" / "full-ci.sh"

RUNNER_STUB = textwrap.dedent(
    """#!/usr/bin/env bash
    set -euo pipefail
    printf '%s\\n' "$*" >> "$STUB_CALL_LOG"
    junit_dir=""
    prev=""
    for arg in "$@"; do
      if [[ "$prev" == "--junit-dir" ]]; then junit_dir="$arg"; fi
      prev="$arg"
    done
    mkdir -p "$junit_dir"
    count="${STUB_JUNIT_COUNT:-2}"
    i=0
    while [[ "$i" -lt "$count" ]]; do
      printf '<testcase name="t%s" classname="c"/>' "$i" > "$junit_dir/pass-$i.xml"
      i=$((i + 1))
    done
    exit "${STUB_RUNNER_RC:-0}"
    """
)


class UnitGateWiringTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        script = FULL_CI.read_text(encoding="utf-8")
        cls.gate_fn = cls._function_body(
            script, "run_known_failures_gate", "\nrun_unit_tests() {"
        )
        cls.unit_fn = cls._function_body(
            script, "run_unit_tests", "\nrun_checker_integration_tests() {"
        )
        cls.checker_fn = cls._function_body(
            script, "run_checker_integration_tests", "\nbuild_unit_test_archive() {"
        )

    @staticmethod
    def _function_body(script, name, end_marker):
        start = script.index(f"{name}() {{")
        end = script.index(end_marker, start)
        return script[start:end]

    def _run(self, entry="run_unit_tests", env_line="", node_rc=0, runner_rc=0, junit_count=2, packages_rc=0):
        tmp = tempfile.mkdtemp(prefix="unit-gate-test-")
        self.addCleanup(__import__("shutil").rmtree, tmp, True)
        stub_dir = pathlib.Path(tmp) / "scripts" / "ci"
        stub_dir.mkdir(parents=True)
        stub = stub_dir / "unit-nextest.sh"
        stub.write_text(RUNNER_STUB, encoding="utf-8")
        stub.chmod(0o755)
        call_log = pathlib.Path(tmp) / "calls.log"
        call_log.write_text("", encoding="utf-8")
        node_log = pathlib.Path(tmp) / "node.log"
        node_log.write_text("", encoding="utf-8")
        harness = textwrap.dedent(
            f"""#!/usr/bin/env bash
            set -Eeuo pipefail
            cd {tmp}
            {env_line}
            export STUB_CALL_LOG={call_log}
            export STUB_RUNNER_RC={runner_rc}
            export STUB_JUNIT_COUNT={junit_count}
            LOG_DIR={tmp}/logs
            mkdir -p "$LOG_DIR"
            ci_section() {{ :; }}
            unit_test_packages() {{
              if [[ {packages_rc} -ne 0 ]]; then
                echo "error: bad override" >&2
                return {packages_rc}
              fi
              printf '%s\\n' tsz-core tsz-checker
            }}
            node() {{ printf '%s\\n' "$*" >> {node_log}; return {node_rc}; }}
            {self.gate_fn}
            {self.unit_fn}
            {self.checker_fn}
            set +e
            {entry}
            rc=$?
            set -e
            echo "RC=$rc"
            """
        )
        proc = subprocess.run(["bash", "-c", harness], capture_output=True, text=True)
        self.assertEqual(
            proc.returncode, 0, msg=f"harness aborted:\n{proc.stdout}\n{proc.stderr}"
        )
        rc_lines = [ln for ln in proc.stdout.splitlines() if ln.startswith("RC=")]
        self.assertEqual(len(rc_lines), 1, msg=f"missing RC line:\n{proc.stdout}")
        return (
            int(rc_lines[0][len("RC=") :]),
            call_log.read_text(encoding="utf-8"),
            node_log.read_text(encoding="utf-8"),
            proc.stdout + proc.stderr,
        )

    def test_green_run_gates_and_passes(self):
        rc, calls, node_calls, _ = self._run()
        self.assertEqual(rc, 0)
        self.assertIn("--packages tsz-core tsz-checker", calls)
        self.assertIn("known-failures-check.mjs --junit-dir", node_calls)

    def test_gate_verdict_is_the_job_verdict(self):
        rc, _, node_calls, _ = self._run(node_rc=1)
        self.assertEqual(rc, 1)
        self.assertIn("known-failures-check.mjs", node_calls)

    def test_infrastructure_failure_never_reaches_the_gate(self):
        rc, _, node_calls, out = self._run(runner_rc=101)
        self.assertEqual(rc, 101)
        self.assertEqual(node_calls, "")
        self.assertIn("infrastructure error", out)

    def test_no_tests_selected_skips_the_gate(self):
        rc, _, node_calls, out = self._run(junit_count=0)
        self.assertEqual(rc, 0)
        self.assertEqual(node_calls, "")
        self.assertIn("skipping known-failures gate", out)

    def test_bad_package_override_propagates(self):
        rc, calls, node_calls, _ = self._run(packages_rc=2)
        self.assertEqual(rc, 2)
        self.assertEqual(calls, "")
        self.assertEqual(node_calls, "")

    def test_skip_checker_integration_flag_is_forwarded(self):
        rc, calls, _, _ = self._run(
            env_line="export TSZ_CI_UNIT_SKIP_CHECKER_INTEGRATION=1"
        )
        self.assertEqual(rc, 0)
        self.assertIn("--skip-checker-integration", calls)

    def test_checker_integration_suite_shares_the_wiring(self):
        rc, calls, node_calls, _ = self._run(entry="run_checker_integration_tests")
        self.assertEqual(rc, 0)
        self.assertIn("--packages tsz-checker", calls)
        self.assertIn("known-failures-check.mjs --junit-dir", node_calls)

    def test_checker_integration_infra_failure_fails(self):
        rc, _, node_calls, _ = self._run(
            entry="run_checker_integration_tests", runner_rc=7
        )
        self.assertEqual(rc, 7)
        self.assertEqual(node_calls, "")


if __name__ == "__main__":
    unittest.main()
