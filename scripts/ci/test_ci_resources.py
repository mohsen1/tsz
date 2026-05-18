"""Tests for scripts/ci/ci-resources.sh.

The module provides hardware resource budget helpers (worker counts, build job
limits) used by gcp-full-ci.sh.  These tests exercise the functions via
subprocess so regressions (wrong arithmetic, division-by-zero on small hosts,
out-of-range worker counts) fail closed in CI.
"""

import os
import pathlib
import subprocess
import unittest
from typing import Optional

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "ci" / "ci-resources.sh"


def call_function(func_name: str, *args: str, host_cpus: int = 8, shard_count: int = 4,
                  env_overrides: Optional[dict[str, str]] = None) -> subprocess.CompletedProcess:
    env = os.environ.copy()
    env["HOST_CPUS"] = str(host_cpus)
    env["SHARD_COUNT"] = str(shard_count)
    if env_overrides:
        env.update(env_overrides)
    arg_str = " ".join(str(a) for a in args)
    cmd = f'source {SCRIPT}; {func_name} {arg_str}'
    return subprocess.run(
        ["bash", "-c", cmd],
        capture_output=True,
        text=True,
        env=env,
        check=False,
    )


def result_int(r: subprocess.CompletedProcess) -> int:
    assert r.returncode == 0, f"exit {r.returncode}: {r.stderr}"
    return int(r.stdout.strip())


class ScriptPresenceTests(unittest.TestCase):
    def test_script_exists(self):
        self.assertTrue(SCRIPT.exists(), f"missing helper: {SCRIPT}")

    def test_syntax_check(self):
        r = subprocess.run(["bash", "-n", str(SCRIPT)], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, msg=r.stderr)


class HostMemoryMbTests(unittest.TestCase):
    def test_returns_non_negative_integer(self):
        r = call_function("host_memory_mb")
        val = result_int(r)
        self.assertGreaterEqual(val, 0)

    def test_returns_positive_on_linux(self):
        if not pathlib.Path("/proc/meminfo").exists():
            self.skipTest("/proc/meminfo not available")
        r = call_function("host_memory_mb")
        val = result_int(r)
        self.assertGreater(val, 0)


class CapWorkersTests(unittest.TestCase):
    def test_below_cap_passes_through(self):
        # requested (3) < HOST_CPUS (8) → returns 3
        self.assertEqual(result_int(call_function("cap_workers", 3, host_cpus=8)), 3)

    def test_equal_to_cap_passes_through(self):
        # requested == HOST_CPUS → returns HOST_CPUS
        self.assertEqual(result_int(call_function("cap_workers", 8, host_cpus=8)), 8)

    def test_above_cap_is_clamped(self):
        # requested (16) > HOST_CPUS (8) → returns 8
        self.assertEqual(result_int(call_function("cap_workers", 16, host_cpus=8)), 8)

    def test_single_cpu_host(self):
        # On a 1-CPU host any request >= 1 is clamped to 1
        self.assertEqual(result_int(call_function("cap_workers", 32, host_cpus=1)), 1)


class DefaultCargoBuildJobsTests(unittest.TestCase):
    def test_returns_at_least_one(self):
        r = call_function("default_cargo_build_jobs")
        self.assertGreaterEqual(result_int(r), 1)

    def test_huge_per_job_mb_forces_one_job(self):
        # With 999 GB per job no machine can run more than one job.
        r = call_function(
            "default_cargo_build_jobs",
            host_cpus=32,
            env_overrides={"TSZ_CI_CARGO_MB_PER_JOB": "999999"},
        )
        self.assertEqual(result_int(r), 1)

    def test_unit_suite_uses_separate_mb_knob(self):
        # unit suite reads TSZ_CI_UNIT_CARGO_MB_PER_JOB, not TSZ_CI_CARGO_MB_PER_JOB.
        # Setting only the non-unit knob to a huge value must not affect unit suite.
        r = call_function(
            "default_cargo_build_jobs",
            host_cpus=32,
            env_overrides={
                "TSZ_CI_SUITE": "unit",
                "TSZ_CI_UNIT_CARGO_MB_PER_JOB": "999999",
                "TSZ_CI_CARGO_MB_PER_JOB": "1",
            },
        )
        self.assertEqual(result_int(r), 1)

    def test_does_not_exceed_host_cpus(self):
        r = call_function("default_cargo_build_jobs", host_cpus=4,
                          env_overrides={"TSZ_CI_CARGO_MB_PER_JOB": "1"})
        self.assertLessEqual(result_int(r), 4)


class DefaultShardWorkersTests(unittest.TestCase):
    def test_returns_at_least_one(self):
        r = call_function("default_shard_workers", host_cpus=8, shard_count=4)
        self.assertGreaterEqual(result_int(r), 1)

    def test_does_not_exceed_host_cpus(self):
        # cap_workers clamps the result to HOST_CPUS.
        r = call_function("default_shard_workers", host_cpus=2, shard_count=4)
        self.assertLessEqual(result_int(r), 2)

    def test_maximum_is_64(self):
        # The function clamps at 64 before cap_workers.
        r = call_function("default_shard_workers", host_cpus=256, shard_count=1)
        self.assertLessEqual(result_int(r), 64)


class DefaultEmitWorkersTests(unittest.TestCase):
    def test_returns_at_least_one(self):
        r = call_function("default_emit_workers", host_cpus=8, shard_count=4)
        self.assertGreaterEqual(result_int(r), 1)

    def test_maximum_is_32(self):
        # emit workers are capped at 32 regardless of CPU count.
        r = call_function("default_emit_workers", host_cpus=256, shard_count=1)
        self.assertLessEqual(result_int(r), 32)

    def test_does_not_exceed_host_cpus(self):
        r = call_function("default_emit_workers", host_cpus=2, shard_count=4)
        self.assertLessEqual(result_int(r), 2)


class DefaultFourslashWorkersTests(unittest.TestCase):
    def test_returns_at_least_two(self):
        r = call_function("default_fourslash_workers", host_cpus=8, shard_count=4)
        self.assertGreaterEqual(result_int(r), 2)

    def test_maximum_is_32(self):
        r = call_function("default_fourslash_workers", host_cpus=256, shard_count=1)
        self.assertLessEqual(result_int(r), 32)

    def test_memory_pressure_caps_workers(self):
        # With 999 GB per worker no machine can run more than 2 workers
        # (mem_cap floors at 2).
        r = call_function(
            "default_fourslash_workers",
            host_cpus=32,
            shard_count=4,
            env_overrides={"TSZ_CI_FOURSLASH_MB_PER_WORKER": "999999"},
        )
        self.assertEqual(result_int(r), 2)

    def test_does_not_exceed_host_cpus(self):
        r = call_function("default_fourslash_workers", host_cpus=2, shard_count=1)
        self.assertLessEqual(result_int(r), 2)


class DefaultConformanceWorkersTests(unittest.TestCase):
    def test_returns_at_least_one(self):
        r = call_function("default_conformance_workers", host_cpus=8, shard_count=4)
        self.assertGreaterEqual(result_int(r), 1)

    def test_maximum_is_128(self):
        r = call_function("default_conformance_workers", host_cpus=256, shard_count=4)
        self.assertLessEqual(result_int(r), 128)

    def test_memory_pressure_floors_at_8(self):
        # With 999 GB per worker mem_cap=0 → clamps to 8, then capped by HOST_CPUS.
        r = call_function(
            "default_conformance_workers",
            host_cpus=32,
            env_overrides={"TSZ_CI_CONFORMANCE_MB_PER_WORKER": "999999"},
        )
        # mem_cap floors at 8, workers started at HOST_CPUS-8=24, capped to 8
        self.assertEqual(result_int(r), 8)

    def test_does_not_exceed_host_cpus(self):
        r = call_function("default_conformance_workers", host_cpus=4,
                          env_overrides={"TSZ_CI_CONFORMANCE_MB_PER_WORKER": "1"})
        self.assertLessEqual(result_int(r), 4)


if __name__ == "__main__":
    unittest.main()
