"""Tests for README metric refresh helpers."""

import importlib.util
import pathlib
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
REFRESH_README = ROOT / "scripts" / "refresh-readme.py"

spec = importlib.util.spec_from_file_location("refresh_readme", REFRESH_README)
refresh_readme = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(refresh_readme)


class RefreshReadmeTests(unittest.TestCase):
    def test_ci_suite_metric_shape_normalizes_to_counts(self):
        summary = refresh_readme.normalize_suite_summary({
            "suite": "conformance",
            "pass_rate": "100.0",
            "passed": 12579,
            "total": 12585,
        }, "conformance")

        self.assertEqual(summary["passed"], 12579)
        self.assertEqual(summary["total"], 12585)

    def test_snapshot_suite_metric_shape_normalizes_to_counts(self):
        summary = refresh_readme.normalize_suite_summary({
            "summary": {
                "passed": 6558,
                "total": 6562,
            },
        }, "fourslash")

        self.assertEqual(summary["passed"], 6558)
        self.assertEqual(summary["total"], 6562)

    def test_ci_emit_metric_shape_normalizes_to_readme_summary(self):
        summary = refresh_readme.normalize_emit_summary({
            "suite": "emit",
            "js_passed": 13401,
            "js_total": 13530,
            "js_skipped": 1,
            "js_timeouts": 0,
            "dts_passed": 1619,
            "dts_total": 1669,
            "dts_skipped": 11862,
        })

        self.assertEqual(summary["jsPass"], 13401)
        self.assertEqual(summary["jsTotal"], 13530)
        self.assertEqual(summary["jsSkip"], 1)
        self.assertEqual(summary["jsTimeout"], 0)
        self.assertEqual(summary["dtsPass"], 1619)
        self.assertEqual(summary["dtsTotal"], 1669)
        self.assertEqual(summary["dtsSkip"], 11862)

    def test_existing_readme_emit_block_is_not_downgraded_by_old_snapshot(self):
        readme_summary = refresh_readme.emit_summary_from_readme(
            """<!-- EMIT_START -->
```
JavaScript:  [████████████████████] 99.0% (13,401 / 13,530 tests)
Declaration: [███████████████████░] 97.0% (1,619 / 1,669 tests)
```
<!-- EMIT_END -->""",
        )
        snapshot_summary = {
            "jsPass": 13094,
            "jsTotal": 13530,
            "dtsPass": 1606,
            "dtsTotal": 1669,
        }

        selected = refresh_readme.prefer_readme_emit_summary(snapshot_summary, readme_summary)

        self.assertEqual(selected["jsPass"], 13401)
        self.assertEqual(selected["dtsPass"], 1619)


if __name__ == "__main__":
    unittest.main()
