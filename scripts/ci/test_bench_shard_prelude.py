"""Tests for scripts/ci/bench-shard-prelude.sh.

The script gates bench shard startup on the self-hosted runner. These tests
exercise its CLI surface so regressions (typos, missing subcommands, broken
exit codes) fail closed in CI.
"""

import os
import pathlib
import subprocess
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "ci" / "bench-shard-prelude.sh"


def run_script(*args: str, env_overrides: dict | None = None, cwd: pathlib.Path | None = None):
    env = os.environ.copy()
    if env_overrides:
        env.update(env_overrides)
    return subprocess.run(
        [str(SCRIPT), *args],
        capture_output=True,
        text=True,
        env=env,
        cwd=str(cwd) if cwd else None,
        check=False,
    )


class BenchShardPreludeCLITests(unittest.TestCase):
    def test_script_is_executable(self):
        self.assertTrue(SCRIPT.exists(), f"missing helper: {SCRIPT}")
        self.assertTrue(os.access(SCRIPT, os.X_OK), f"{SCRIPT} is not executable")

    def test_no_subcommand_exits_with_usage(self):
        result = run_script()
        self.assertEqual(result.returncode, 2, msg=result.stderr)
        self.assertIn("Usage:", result.stdout + result.stderr)

    def test_unknown_subcommand_exits_two(self):
        result = run_script("garbage")
        self.assertEqual(result.returncode, 2, msg=result.stderr)
        self.assertIn("unknown subcommand", result.stderr)

    def test_unknown_argument_exits_two(self):
        result = run_script("prelude", "--banana", "yellow")
        self.assertEqual(result.returncode, 2, msg=result.stderr)
        self.assertIn("unknown argument", result.stderr)

    def test_help_flag_prints_usage(self):
        result = run_script("--help")
        # --help exits 0 (it is a request for documentation, not an error).
        self.assertEqual(result.returncode, 0, msg=result.stderr)
        self.assertIn("prelude", result.stdout)
        self.assertIn("postmortem", result.stdout)


class BenchShardPreludeSubcommandTests(unittest.TestCase):
    def test_prelude_reports_state_and_succeeds(self):
        # MIN_FREE_MB=1 is small enough that any healthy host satisfies the
        # floor, so the prelude exits 0 even on tight CI machines.
        result = run_script(
            "prelude",
            "--label",
            "unit-test",
            env_overrides={"TSZ_BENCH_MIN_FREE_MB": "1"},
        )
        self.assertEqual(result.returncode, 0, msg=result.stderr)
        out = result.stdout
        self.assertIn("prelude (pre-cleanup)", out)
        self.assertIn("unit-test", out)
        # No orphans should exist in a clean unittest invocation, so cleanup
        # reports zero and the post-cleanup state report is skipped.
        self.assertIn("orphans_terminated=0", out)
        self.assertNotIn("prelude (post-cleanup)", out)
        # Memory floor diagnostics are emitted even when the floor passes.
        self.assertIn("MemAvailable_MB=", out)

    def test_prelude_refuses_when_floor_not_met(self):
        # A huge floor (2 PiB) cannot be met on any realistic runner, so the
        # prelude must refuse to start with EX_TEMPFAIL (75).
        result = run_script(
            "prelude",
            "--label",
            "starved",
            env_overrides={"TSZ_BENCH_MIN_FREE_MB": "2147483647"},
        )
        self.assertEqual(result.returncode, 75, msg=result.stderr)
        self.assertIn("refusing to start", result.stderr)

    def test_postmortem_writes_default_output_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            cwd = pathlib.Path(tmp)
            result = run_script("postmortem", "--label", "deadbeef", cwd=cwd)
            self.assertEqual(result.returncode, 0, msg=result.stderr)
            self.assertIn("wrote postmortem", result.stdout)
            expected = cwd / "bench-postmortem-deadbeef.log"
            self.assertTrue(expected.exists(), f"missing postmortem at {expected}")
            content = expected.read_text()
            self.assertIn("postmortem", content)
            self.assertIn("deadbeef", content)
            # Disk + memory snapshot must be present so the postmortem is
            # diagnostically useful even when dmesg is unreadable.
            self.assertIn("memory (free -h)", content)
            self.assertIn("disk (df -h", content)

    def test_postmortem_honors_explicit_output_path(self):
        with tempfile.TemporaryDirectory() as tmp:
            out_path = pathlib.Path(tmp) / "subdir" / "custom.log"
            out_path.parent.mkdir(parents=True, exist_ok=True)
            result = run_script(
                "postmortem", "--label", "x", "--output", str(out_path)
            )
            self.assertEqual(result.returncode, 0, msg=result.stderr)
            self.assertTrue(out_path.exists())
            self.assertIn("postmortem", out_path.read_text())


if __name__ == "__main__":
    unittest.main()
