#!/usr/bin/env python3
import json
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("query-perf-counters.py")


def sample_snapshot(*, type_environment_core: int = 7, import_type: int = 3):
    return {
        "schema_version": 2,
        "mode": "attribution",
        "enabled": True,
        "delegate": {
            "calls": 12,
            "cache_hits_lib": 4,
            "cache_hits_cross_file": 5,
            "misses": 3,
            "max_recursion_depth": 2,
            "cross_file_type_params_cache_hits": 8,
            "cross_file_type_params_cache_misses": 2,
        },
        "checker": {
            "state_constructed": 20,
            "with_parent_cache_constructed": type_environment_core + import_type,
            "file_session_resets": 2,
            "compute_type_of_symbol_calls": 30,
            "compute_type_of_symbol_cache_hits": 10,
        },
        "overlay": {
            "copy_calls": 4,
            "entries_total": 120,
            "entries_max": 50,
        },
        "resolver": {
            "lookup_calls": 40,
            "is_file_calls": 9,
            "is_dir_calls": 6,
            "package_json_reads": 3,
        },
        "interner": {
            "intern_calls": 100,
            "intern_hits": 75,
            "intern_misses": 25,
            "lock_wait_histogram_ns": None,
        },
        "by_reason": [
            {
                "reason": "TypeEnvironmentCore",
                "with_parent_cache_constructed": type_environment_core,
                "overlay_copy_calls": 2,
                "overlay_copy_entries": 70,
                "overlay_copy_max_entries": 40,
            },
            {
                "reason": "ImportType",
                "with_parent_cache_constructed": import_type,
                "overlay_copy_calls": 1,
                "overlay_copy_entries": 30,
                "overlay_copy_max_entries": 30,
            },
        ],
    }


class QueryPerfCountersTests(unittest.TestCase):
    def run_tool(self, *args):
        return subprocess.run(
            ["python3", str(SCRIPT), *map(str, args)],
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

    def write_json(self, root: Path, name: str, payload: dict):
        path = root / name
        path.write_text(json.dumps(payload), encoding="utf-8")
        return path

    def test_default_summary_prints_key_sections_and_dominant_reason(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = self.write_json(Path(temp_dir), "snap.json", sample_snapshot())
            result = self.run_tool("--json", path)

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("perf-counter JSON:", result.stdout)
        self.assertIn("delegate (cross-arena symbol resolution):", result.stdout)
        self.assertIn("Dominant: TypeEnvironmentCore = 7", result.stdout)
        self.assertIn("Top non-baseline T2.2 target: ImportType = 3", result.stdout)

    def test_by_reason_prints_only_reason_table(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = self.write_json(Path(temp_dir), "snap.json", sample_snapshot())
            result = self.run_tool("--json", path, "--by-reason")

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("reason", result.stdout)
        self.assertIn("ImportType", result.stdout)
        self.assertNotIn("delegate (cross-arena symbol resolution):", result.stdout)

    def test_baseline_diff_marks_improvements_and_regressions(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            baseline = self.write_json(
                root,
                "baseline.json",
                sample_snapshot(type_environment_core=10, import_type=2),
            )
            current = self.write_json(
                root,
                "current.json",
                sample_snapshot(type_environment_core=6, import_type=5),
            )
            result = self.run_tool("--json", current, "--baseline", baseline)

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("with_parent_cache_constructed:", result.stdout)
        self.assertIn("(-1)", result.stdout)
        self.assertIn("TypeEnvironmentCore", result.stdout)
        self.assertIn("improved", result.stdout)
        self.assertIn("ImportType", result.stdout)
        self.assertIn("regressed", result.stdout)

    def test_by_reason_requires_by_reason_rows(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            payload = sample_snapshot()
            del payload["by_reason"]
            path = self.write_json(Path(temp_dir), "old.json", payload)
            result = self.run_tool("--json", path, "--by-reason")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("missing `by_reason`", result.stderr)

    def test_missing_json_file_exits_nonzero(self):
        result = self.run_tool("--json", "/tmp/definitely-missing-tsz-perf.json")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("perf-counter JSON not found", result.stderr)


if __name__ == "__main__":
    unittest.main()
