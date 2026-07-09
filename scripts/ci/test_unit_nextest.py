"""Contract tests for scripts/ci/unit-nextest.sh (#15646).

The script runs the delta-verified unit suite: a general pass plus batched
tsz-checker [[test]] targets, one junit per pass collected into --junit-dir,
optionally followed by the known-failures gate (--gate). Its exit code must
distinguish "tests ran and were recorded" (0, even with test failures) from
infrastructure failures (build error, missing junit) — that split is what
lets known-failures-check.mjs be the only pass/fail gate while batch masking
(#15404) cannot return. Batch binaries are pruned after collection so peak
disk stays bounded (they measure ~78 MB each; 787 targets would be ~61 GB).

The tests copy the script into a temp workspace whose `cargo` and `node` are
stubs, so every invocation and its side effects are scripted.
"""

import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import textwrap
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "ci" / "unit-nextest.sh"

# The stub cargo:
#  * `cargo metadata` prints a minimal metadata document declaring five
#    tsz-checker [[test]] targets and a scratch target directory;
#  * `cargo nextest run` logs its full argument list (one line per call) to
#    CALL_LOG, then consults FAIL_SPEC — lines of `token rc [nojunit]` — and
#    exits with `rc` for the first token found among its arguments (writing
#    the junit report first unless `nojunit` is present or rc is 4). It also
#    deposits a fake test binary for every `--test` target so pruning is
#    observable.
CARGO_STUB = textwrap.dedent(
    """#!/usr/bin/env bash
    set -euo pipefail
    if [[ "$1" == "metadata" ]]; then
      cat "$METADATA_FIXTURE"
      exit 0
    fi
    printf '%s\\n' "$*" >> "$CALL_LOG"
    mkdir -p "$WORKSPACE/.target/ci-unit/deps"
    prev=""
    for arg in "$@"; do
      if [[ "$prev" == "--test" ]]; then
        touch "$WORKSPACE/.target/ci-unit/deps/$arg-cafe0123"
      fi
      prev="$arg"
    done
    write_junit=1
    rc=0
    while read -r token spec_rc nojunit; do
      [[ -n "$token" ]] || continue
      for arg in "$@"; do
        if [[ "$arg" == "$token" ]]; then
          rc="$spec_rc"
          if [[ "$nojunit" == "nojunit" || "$spec_rc" == "4" ]]; then
            write_junit=0
          fi
          break 2
        fi
      done
    done < "$FAIL_SPEC"
    if [[ "$write_junit" == "1" ]]; then
      mkdir -p "$WORKSPACE/target/nextest/signoff"
      printf '<testcase name="t" classname="c"/>' > "$WORKSPACE/target/nextest/signoff/junit.xml"
    fi
    exit "$rc"
    """
)

NODE_STUB = textwrap.dedent(
    """#!/usr/bin/env bash
    printf '%s\\n' "$*" >> "$NODE_LOG"
    exit "${NODE_STUB_RC:-0}"
    """
)


class UnitNextestTests(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.mkdtemp(prefix="unit-nextest-test-")
        self.addCleanup(shutil.rmtree, tmp, True)
        self.tmp = pathlib.Path(tmp)
        (self.tmp / "scripts" / "ci").mkdir(parents=True)
        self.script = self.tmp / "scripts" / "ci" / "unit-nextest.sh"
        self.script.write_bytes(SCRIPT.read_bytes())
        self.script.chmod(0o755)
        bindir = self.tmp / "bin"
        bindir.mkdir()
        for name, stub in (("cargo", CARGO_STUB), ("node", NODE_STUB)):
            path = bindir / name
            path.write_text(stub, encoding="utf-8")
            path.chmod(0o755)
        self.call_log = self.tmp / "calls.log"
        self.call_log.write_text("", encoding="utf-8")
        self.node_log = self.tmp / "node.log"
        self.node_log.write_text("", encoding="utf-8")
        self.fail_spec = self.tmp / "fail.spec"
        self.fail_spec.write_text("", encoding="utf-8")
        metadata = {
            "packages": [
                {
                    "name": "tsz-checker",
                    "targets": [
                        {"kind": ["test"], "name": name}
                        for name in ["t_a", "t_b", "t_c", "t_d", "t_e"]
                    ]
                    + [{"kind": ["lib"], "name": "tsz-checker"}],
                }
            ],
            "target_directory": str(self.tmp / ".target"),
        }
        self.metadata_fixture = self.tmp / "metadata.json"
        self.metadata_fixture.write_text(json.dumps(metadata), encoding="utf-8")
        self.junit_dir = self.tmp / "junit"

    def run_script(self, *args, fail_spec="", extra_env=None):
        self.fail_spec.write_text(fail_spec, encoding="utf-8")
        env = dict(os.environ)
        env["PATH"] = f"{self.tmp / 'bin'}{os.pathsep}{env['PATH']}"
        env["METADATA_FIXTURE"] = str(self.metadata_fixture)
        env["CALL_LOG"] = str(self.call_log)
        env["NODE_LOG"] = str(self.node_log)
        env["FAIL_SPEC"] = str(self.fail_spec)
        env["WORKSPACE"] = str(self.tmp)
        env["TSZ_CI_CHECKER_TEST_BATCH_SIZE"] = "2"
        for var in ("CARGO_BUILD_JOBS", "UNIT_NEXTEST_TEST_THREADS", "CHECKER_NEXTEST_TEST_THREADS"):
            env.pop(var, None)
        if extra_env:
            env.update(extra_env)
        return subprocess.run(
            [str(self.script), "--junit-dir", str(self.junit_dir), *args],
            capture_output=True,
            text=True,
            env=env,
        )

    def calls(self):
        return self.call_log.read_text(encoding="utf-8").splitlines()

    def node_calls(self):
        return self.node_log.read_text(encoding="utf-8").splitlines()

    def junits(self):
        if not self.junit_dir.exists():
            return []
        return sorted(p.name for p in self.junit_dir.glob("*.xml"))

    def deps(self):
        deps_dir = self.tmp / ".target" / "ci-unit" / "deps"
        if not deps_dir.exists():
            return []
        return sorted(p.name for p in deps_dir.iterdir())

    def test_green_run_collects_one_junit_per_pass(self):
        result = self.run_script("--packages", "tsz-core tsz-checker")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        # one general pass + ceil(5/2) = 3 checker batches
        self.assertEqual(
            self.junits(),
            ["checker-01.xml", "checker-02.xml", "checker-03.xml", "general.xml"],
        )
        calls = self.calls()
        self.assertEqual(len(calls), 4)
        self.assertIn("-p tsz-core", calls[0])
        self.assertNotIn("tsz-checker", calls[0])
        self.assertIn("--test t_a --test t_b", calls[1])
        self.assertIn("--test t_e", calls[3])
        # the checker lib-test must never be built: every checker pass names
        # explicit --test targets
        for call in calls[1:]:
            self.assertIn("-p tsz-checker --test", call)
        # without --gate the checker is not invoked
        self.assertEqual(self.node_calls(), [])

    def test_batch_binaries_are_pruned_by_default(self):
        result = self.run_script("--packages", "tsz-checker")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.deps(), [])

    def test_keep_batch_artifacts_skips_pruning(self):
        result = self.run_script("--packages", "tsz-checker", "--keep-batch-artifacts")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(len(self.deps()), 5)

    def test_gate_runs_known_failures_check_and_propagates_verdict(self):
        ok = self.run_script("--packages", "tsz-core tsz-checker", "--gate")
        self.assertEqual(ok.returncode, 0, ok.stdout + ok.stderr)
        self.assertEqual(len(self.node_calls()), 1)
        self.assertIn("known-failures-check.mjs --junit-dir", self.node_calls()[0])
        self.assertNotIn("--allow-no-reports", self.node_calls()[0])
        red = self.run_script(
            "--packages", "tsz-core tsz-checker", "--gate",
            extra_env={"NODE_STUB_RC": "1"},
        )
        self.assertEqual(red.returncode, 1, red.stdout + red.stderr)

    def test_gate_forwards_allow_no_reports(self):
        result = self.run_script(
            "--packages", "tsz-core tsz-checker", "--gate", "--allow-no-reports"
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("--allow-no-reports", self.node_calls()[0])

    def test_test_failures_are_recorded_not_fatal(self):
        result = self.run_script(
            "--packages", "tsz-core tsz-checker", fail_spec="t_c 100\n"
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(len(self.junits()), 4)
        self.assertIn("1 pass(es) with test failures", result.stdout)
        self.assertIn("checker-02=100", result.stdout)

    def test_infrastructure_failure_stops_the_run_before_the_gate(self):
        result = self.run_script(
            "--packages", "tsz-core tsz-checker", "--gate", fail_spec="t_c 101 nojunit\n"
        )
        self.assertEqual(result.returncode, 101, result.stdout + result.stderr)
        # general + batch 1 succeeded; batch 2 failed; batch 3 never ran
        self.assertEqual(len(self.calls()), 3)
        self.assertEqual(self.junits(), ["checker-01.xml", "general.xml"])
        self.assertEqual(self.node_calls(), [])

    def test_missing_junit_after_green_pass_is_infra(self):
        result = self.run_script(
            "--packages", "tsz-core", fail_spec="tsz-core 0 nojunit\n"
        )
        self.assertEqual(result.returncode, 3, result.stdout + result.stderr)

    def test_no_tests_selected_is_not_an_error(self):
        result = self.run_script(
            "--packages", "tsz-core", fail_spec="tsz-core 4\n"
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.junits(), [])
        self.assertIn("selected no tests", result.stdout)

    def test_skip_checker_integration_runs_only_general(self):
        result = self.run_script(
            "--packages", "tsz-core tsz-checker", "--skip-checker-integration"
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.junits(), ["general.xml"])

    def test_workspace_minus_checker_general_pass(self):
        result = self.run_script("--workspace-minus-checker")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        calls = self.calls()
        self.assertIn("--workspace --exclude tsz-checker", calls[0])
        # checker batches still run
        self.assertEqual(len(calls), 4)

    def test_packages_and_workspace_flags_are_mutually_exclusive(self):
        result = self.run_script("--packages", "tsz-core", "--workspace-minus-checker")
        self.assertEqual(result.returncode, 2)

    def test_build_jobs_and_thread_caps_are_forwarded(self):
        result = self.run_script(
            "--packages",
            "tsz-core tsz-checker",
            extra_env={
                "CARGO_BUILD_JOBS": "7",
                "UNIT_NEXTEST_TEST_THREADS": "3",
                "CHECKER_NEXTEST_TEST_THREADS": "5",
            },
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        calls = self.calls()
        self.assertIn("--build-jobs 7", calls[0])
        self.assertIn("--test-threads 3", calls[0])
        # checker batches get their own cap (hosted-runner memory policy from
        # the retired [profile.ci]) plus the build-jobs cap
        self.assertIn("--build-jobs 7", calls[1])
        self.assertIn("--test-threads 5", calls[1])
        self.assertNotIn("--test-threads 3", calls[1])


if __name__ == "__main__":
    unittest.main()
