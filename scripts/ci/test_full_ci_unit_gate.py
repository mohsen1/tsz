"""Contract tests for the unit-lane known-failures gate wiring in full-ci.sh.

Successor to the #15404 rc-aggregation tests (test_full_ci_unit_rc.py, removed
with the #15647 re-mask): since #15646 both unit lanes are one call to
scripts/ci/unit-nextest.sh with --gate, which collects one junit per nextest
pass and has known-failures-check.mjs adjudicate (see test_unit_nextest.py for
the runner's own contract). These tests extract the wiring functions verbatim
from full-ci.sh, stub the runner, and assert:

* the runner is invoked with --gate --allow-no-reports and the resolved
  package set, so the delta gate is the job's verdict;
* the runner's rc (infra failure or gate verdict) propagates unchanged;
* narrow-override validation errors propagate and skip the runner;
* the skip-checker-integration knob is forwarded;
* the checker-integration suite shares the same wiring.
"""

import pathlib
import shutil
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
    exit "${STUB_RUNNER_RC:-0}"
    """
)


class UnitGateWiringTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        script = FULL_CI.read_text(encoding="utf-8")
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

    def _run(self, entry="run_unit_tests", env_line="", runner_rc=0, packages_rc=0):
        tmp = tempfile.mkdtemp(prefix="unit-gate-test-")
        self.addCleanup(shutil.rmtree, tmp, True)
        stub_dir = pathlib.Path(tmp) / "scripts" / "ci"
        stub_dir.mkdir(parents=True)
        stub = stub_dir / "unit-nextest.sh"
        stub.write_text(RUNNER_STUB, encoding="utf-8")
        stub.chmod(0o755)
        call_log = pathlib.Path(tmp) / "calls.log"
        call_log.write_text("", encoding="utf-8")
        harness = textwrap.dedent(
            f"""#!/usr/bin/env bash
            set -Eeuo pipefail
            cd {tmp}
            {env_line}
            export STUB_CALL_LOG={call_log}
            export STUB_RUNNER_RC={runner_rc}
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
            proc.stdout + proc.stderr,
        )

    def test_unit_lane_invokes_the_gated_runner(self):
        rc, calls, _ = self._run()
        self.assertEqual(rc, 0)
        self.assertIn("--gate", calls)
        # newline-separated package list arrives as one --packages argument
        self.assertIn("--packages tsz-core\ntsz-checker", calls)
        # the full lane always has tests, so zero junit reports must stay an
        # infrastructure failure — no blanket --allow-no-reports
        self.assertNotIn("--allow-no-reports", calls)

    def test_narrowed_override_allows_a_no_tests_run(self):
        rc, calls, _ = self._run(
            env_line="export _TSZ_CI_UNIT_PACKAGES_OVERRIDE='tsz-core'"
        )
        self.assertEqual(rc, 0)
        self.assertIn("--allow-no-reports", calls)

    def test_runner_verdict_is_the_job_verdict(self):
        # rc=1 is the gate's new-failure verdict; rc=101 an infra failure —
        # both propagate unchanged (the runner owns the distinction).
        for rc_in in (1, 101):
            rc, calls, _ = self._run(runner_rc=rc_in)
            self.assertEqual(rc, rc_in)
            self.assertIn("--gate", calls)

    def test_bad_package_override_propagates_and_skips_the_runner(self):
        rc, calls, _ = self._run(packages_rc=2)
        self.assertEqual(rc, 2)
        self.assertEqual(calls, "")

    def test_skip_checker_integration_flag_is_forwarded(self):
        rc, calls, _ = self._run(
            env_line="export TSZ_CI_UNIT_SKIP_CHECKER_INTEGRATION=1"
        )
        self.assertEqual(rc, 0)
        self.assertIn("--skip-checker-integration", calls)

    def test_checker_integration_suite_shares_the_wiring(self):
        rc, calls, _ = self._run(entry="run_checker_integration_tests")
        self.assertEqual(rc, 0)
        self.assertIn("--gate --packages tsz-checker", calls)
        self.assertNotIn("--allow-no-reports", calls)

    def test_checker_integration_runner_rc_propagates(self):
        rc, _, _ = self._run(entry="run_checker_integration_tests", runner_rc=7)
        self.assertEqual(rc, 7)


KNOWN_FAILURES = ROOT / "scripts" / "ci" / "known-failures.txt"
CRATES_DIR = ROOT / "crates"


def _baseline_packages():
    """Packages named by `scripts/ci/known-failures.txt` entries.

    A baseline line is `binary-id::test-name`, and nextest's binary-id always
    starts with the package name, so the first `::` segment is the owner.
    """
    packages = set()
    for line in KNOWN_FAILURES.read_text(encoding="utf-8").splitlines():
        entry = line.strip()
        if not entry or entry.startswith("#"):
            continue
        packages.add(entry.split("::", 1)[0])
    return packages


def _workspace_packages():
    names = set()
    for manifest in CRATES_DIR.glob("*/Cargo.toml"):
        for line in manifest.read_text(encoding="utf-8").splitlines():
            stripped = line.strip()
            if stripped.startswith("name"):
                _, _, value = stripped.partition("=")
                names.add(value.strip().strip('"'))
                break
    return names


class UnitLaneCoverageTests(unittest.TestCase):
    """The lane must actually adjudicate every package the baseline names.

    #15999 §3: `tsz-cli` and `tsz-conformance` sat in no unit lane, so the six
    `tsz-cli` baseline entries were inert — CI never ran that package, its
    tests reached no junit, and known-failures-check.mjs treated them as
    neither a new failure nor a shrink. They read like coverage and were not.
    """

    @classmethod
    def setUpClass(cls):
        script = FULL_CI.read_text(encoding="utf-8")
        start = script.index("_UNIT_TEST_PACKAGES=(")
        end = script.index("\n}", script.index("unit_test_packages() {")) + 2
        cls.packages_fn = script[start:end]

    def _resolve(self, env_line="", expect_rc=0):
        harness = textwrap.dedent(
            f"""#!/usr/bin/env bash
            set -Eeuo pipefail
            {env_line}
            {self.packages_fn}
            set +e
            unit_test_packages
            echo "RC=$?"
            """
        )
        proc = subprocess.run(["bash", "-c", harness], capture_output=True, text=True)
        lines = proc.stdout.splitlines()
        rc_lines = [ln for ln in lines if ln.startswith("RC=")]
        self.assertEqual(len(rc_lines), 1, msg=f"missing RC line:\n{proc.stdout}")
        rc = int(rc_lines[0][len("RC=") :])
        self.assertEqual(rc, expect_rc, msg=proc.stdout + proc.stderr)
        return [ln for ln in lines if not ln.startswith("RC=")]

    def test_known_failure_packages_are_in_the_unit_lane(self):
        lane = set(self._resolve())
        uncovered = sorted(_baseline_packages() - lane)
        self.assertEqual(
            uncovered,
            [],
            msg=(
                "known-failures.txt names packages the unit lane never runs: "
                f"{uncovered}. Their entries can never fail the gate nor "
                "ratchet down. Add them to _UNIT_TEST_PACKAGES in "
                "scripts/ci/full-ci.sh, or drop the entries."
            ),
        )

    def test_lane_packages_exist_in_the_workspace(self):
        workspace = _workspace_packages()
        unknown = sorted(set(self._resolve()) - workspace)
        self.assertEqual(
            unknown,
            [],
            msg=f"_UNIT_TEST_PACKAGES names non-workspace crates: {unknown}",
        )

    def test_override_accepts_every_lane_package(self):
        # The validator's `known` list is derived from _UNIT_TEST_PACKAGES; a
        # hand-copied second literal drifts and rejects lane crates as unknown.
        lane = self._resolve()
        for crate in lane:
            with self.subTest(crate=crate):
                self.assertEqual(
                    self._resolve(
                        env_line=f"export _TSZ_CI_UNIT_PACKAGES_OVERRIDE='{crate}'"
                    ),
                    [crate],
                )

    def test_override_still_rejects_an_unknown_crate(self):
        self._resolve(
            env_line="export _TSZ_CI_UNIT_PACKAGES_OVERRIDE='tsz-not-a-crate'",
            expect_rc=2,
        )


if __name__ == "__main__":
    unittest.main()
