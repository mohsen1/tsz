"""Tests for README metric refresh helpers."""

import io
import importlib.util
import json
import pathlib
import types
import unittest
from contextlib import redirect_stderr
from tempfile import TemporaryDirectory


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

    def test_emit_fallback_warns_when_preserving_readme_summary(self):
        with TemporaryDirectory() as temp_dir:
            root = pathlib.Path(temp_dir)
            detail_path = root / "scripts" / "emit" / "emit-detail.json"
            detail_path.parent.mkdir(parents=True)
            detail_path.write_text(json.dumps({
                "summary": {
                    "jsPass": 13094,
                    "jsTotal": 13530,
                    "dtsPass": 1606,
                    "dtsTotal": 1669,
                },
            }))
            args = types.SimpleNamespace(emit_metrics_json=None)
            readme_text = """<!-- EMIT_START -->
```
JavaScript:  [████████████████████] 99.5% (13,459 / 13,530 tests)
Declaration: [████████████████████] 98.5% (1,644 / 1,669 tests)
```
<!-- EMIT_END -->"""

            old_root = refresh_readme.ROOT
            stderr = io.StringIO()
            try:
                refresh_readme.ROOT = root
                with redirect_stderr(stderr):
                    selected = refresh_readme.load_emit(args, readme_text)
            finally:
                refresh_readme.ROOT = old_root

        self.assertEqual(selected["jsPass"], 13459)
        self.assertEqual(selected["dtsPass"], 1644)
        self.assertIn("preserving README emit metrics", stderr.getvalue())
        self.assertIn("scripts/emit/emit-detail.json", stderr.getvalue())


if __name__ == "__main__":
    unittest.main()
