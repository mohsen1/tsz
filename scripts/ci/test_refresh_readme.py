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
            "pass_rate": "90.0",
            "passed": 9,
            "total": 10,
            "candidates": 12,
            "runnable": 10,
            "unsupported": 1,
            "skipped": 1,
        }, "conformance")

        self.assertEqual(summary["passed"], 9)
        self.assertEqual(summary["total"], 10)
        self.assertEqual(summary["runnable"], 10)
        self.assertEqual(summary["candidates"], 12)
        self.assertEqual(summary["unsupported"], 1)
        self.assertEqual(summary["skipped"], 1)

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

        # A snapshot with no recorded tree (git_sha) is treated as possibly
        # stale, so the higher README block is kept.
        with redirect_stderr(io.StringIO()):
            selected = refresh_readme.resolve_published_summary(
                snapshot_summary, readme_summary, {}, refresh_readme.ROOT / "emit-detail.json",
                is_ahead=refresh_readme.readme_emit_summary_is_ahead,
                suite_label="emit",
            )

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

    def test_existing_readme_conformance_block_is_not_downgraded_by_old_snapshot(self):
        readme_summary = refresh_readme.conformance_summary_from_readme(
            """<!-- CONFORMANCE_START -->
```
Progress: [███████████████████░] 90.0% (9/10 tests)
```
<!-- CONFORMANCE_END -->""",
        )
        snapshot_summary = {
            "passed": 8,
            "total": 9,
        }

        # No recorded tree -> possibly stale -> the higher README block wins.
        with redirect_stderr(io.StringIO()):
            selected = refresh_readme.resolve_published_summary(
                snapshot_summary, readme_summary, {}, refresh_readme.ROOT / "conformance-snapshot.json",
                is_ahead=refresh_readme.readme_suite_summary_is_ahead,
                suite_label="conformance",
            )

        self.assertEqual(selected["passed"], 9)
        self.assertEqual(selected["total"], 10)

    def test_partition_aware_artifact_replaces_legacy_readme_total(self):
        readme_summary = refresh_readme.conformance_summary_from_readme(
            """<!-- CONFORMANCE_START -->
```
Progress: [████████████████████] 100.0% (10/10 tests)
```
<!-- CONFORMANCE_END -->""",
        )
        artifact_summary = {
            "passed": 8,
            "total": 8,
            "runnable": 8,
            "candidates": 10,
            "unsupported": 1,
            "skipped": 1,
        }

        # The partition-aware artifact carries a candidate domain the legacy
        # README block lacks, so it is NOT ahead and the artifact is published.
        with redirect_stderr(io.StringIO()):
            selected = refresh_readme.resolve_published_summary(
                artifact_summary, readme_summary, {}, refresh_readme.ROOT / "conformance-snapshot.json",
                is_ahead=refresh_readme.readme_suite_summary_is_ahead,
                suite_label="conformance",
            )

        self.assertEqual(selected, artifact_summary)

    def test_conformance_readme_parser_reads_candidate_partition(self):
        summary = refresh_readme.conformance_summary_from_readme(
            """<!-- CONFORMANCE_START -->
```
Progress: [████████████████████] 100.0% (8/8 runnable tests)
Candidates: 10 (8 runnable, 1 unsupported, 1 skipped)
```
<!-- CONFORMANCE_END -->""",
        )

        self.assertEqual(
            summary,
            {
                "passed": 8,
                "total": 8,
                "runnable": 8,
                "candidates": 10,
                "unsupported": 1,
                "skipped": 1,
            },
        )

    def test_conformance_fallback_warns_when_preserving_readme_summary(self):
        with TemporaryDirectory() as temp_dir:
            root = pathlib.Path(temp_dir)
            snapshot_path = root / "scripts" / "conformance" / "conformance-snapshot.json"
            snapshot_path.parent.mkdir(parents=True)
            snapshot_path.write_text(json.dumps({
                "summary": {
                    "passed": 8,
                    "total_tests": 8,
                },
            }))
            args = types.SimpleNamespace(conformance_metrics_json=None)
            readme_text = """<!-- CONFORMANCE_START -->
```
Progress: [████████████████████] 100.0% (10/10 tests)
```
<!-- CONFORMANCE_END -->"""

            old_root = refresh_readme.ROOT
            stderr = io.StringIO()
            try:
                refresh_readme.ROOT = root
                with redirect_stderr(stderr):
                    selected = refresh_readme.load_conformance(args, readme_text)
            finally:
                refresh_readme.ROOT = old_root

        self.assertEqual(selected["passed"], 10)
        self.assertEqual(selected["total"], 10)
        self.assertIn("preserving README conformance metrics", stderr.getvalue())
        self.assertIn("scripts/conformance/conformance-snapshot.json", stderr.getvalue())

    def test_artifact_sha_reads_recorded_tree_and_rejects_unknown(self):
        self.assertEqual(
            refresh_readme.artifact_sha({"git_sha": "abc1234def"}), "abc1234def"
        )
        self.assertEqual(
            refresh_readme.artifact_sha({"git_sha": " abc1234def "}), "abc1234def"
        )
        self.assertIsNone(refresh_readme.artifact_sha({"git_sha": "unknown"}))
        self.assertIsNone(refresh_readme.artifact_sha({"git_sha": ""}))
        self.assertIsNone(refresh_readme.artifact_sha({}))
        self.assertIsNone(refresh_readme.artifact_sha(None))

    def test_artifact_describes_current_tree_matches_full_and_short_sha(self):
        head = "0123456789abcdef0123456789abcdef01234567"
        # Exact match.
        self.assertTrue(
            refresh_readme.artifact_describes_current_tree({"git_sha": head}, head)
        )
        # Abbreviated recorded SHA still identifies the same commit.
        self.assertTrue(
            refresh_readme.artifact_describes_current_tree(
                {"git_sha": head[:12]}, head
            )
        )
        # A different commit is not the current tree.
        self.assertFalse(
            refresh_readme.artifact_describes_current_tree(
                {"git_sha": "fedcba9876543210"}, head
            )
        )
        # Too few shared characters to trust as an identity.
        self.assertFalse(
            refresh_readme.artifact_describes_current_tree({"git_sha": "012"}, head)
        )
        # No HEAD (not a git checkout) -> fall back to the magnitude guard.
        self.assertFalse(
            refresh_readme.artifact_describes_current_tree({"git_sha": head}, None)
        )
        # No recorded SHA -> fall back to the magnitude guard.
        self.assertFalse(
            refresh_readme.artifact_describes_current_tree({}, head)
        )

    def test_conformance_zero_drift_publishes_downward_reading(self):
        head = "0123456789abcdef0123456789abcdef01234567"
        with TemporaryDirectory() as temp_dir:
            root = pathlib.Path(temp_dir)
            snapshot_path = root / "scripts" / "conformance" / "conformance-snapshot.json"
            snapshot_path.parent.mkdir(parents=True)
            snapshot_path.write_text(json.dumps({
                "git_sha": head,
                "summary": {"passed": 8, "total_tests": 10},
            }))
            args = types.SimpleNamespace(conformance_metrics_json=None)
            readme_text = """<!-- CONFORMANCE_START -->
```
Progress: [██████████████████░░] 90.0% (9/10 tests)
```
<!-- CONFORMANCE_END -->"""

            old_root = refresh_readme.ROOT
            old_head = refresh_readme.current_head_sha
            stderr = io.StringIO()
            try:
                refresh_readme.ROOT = root
                refresh_readme.current_head_sha = lambda: head
                with redirect_stderr(stderr):
                    selected = refresh_readme.load_conformance(args, readme_text)
            finally:
                refresh_readme.ROOT = old_root
                refresh_readme.current_head_sha = old_head

        # The lower, current-tree reading wins over the README's higher block.
        self.assertEqual(selected["passed"], 8)
        self.assertEqual(selected["total"], 10)
        self.assertIn("current HEAD", stderr.getvalue())

    def test_conformance_stale_artifact_still_preserves_readme_reading(self):
        with TemporaryDirectory() as temp_dir:
            root = pathlib.Path(temp_dir)
            snapshot_path = root / "scripts" / "conformance" / "conformance-snapshot.json"
            snapshot_path.parent.mkdir(parents=True)
            snapshot_path.write_text(json.dumps({
                "git_sha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "summary": {"passed": 8, "total_tests": 10},
            }))
            args = types.SimpleNamespace(conformance_metrics_json=None)
            readme_text = """<!-- CONFORMANCE_START -->
```
Progress: [██████████████████░░] 90.0% (9/10 tests)
```
<!-- CONFORMANCE_END -->"""

            old_root = refresh_readme.ROOT
            old_head = refresh_readme.current_head_sha
            stderr = io.StringIO()
            try:
                refresh_readme.ROOT = root
                refresh_readme.current_head_sha = (
                    lambda: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                )
                with redirect_stderr(stderr):
                    selected = refresh_readme.load_conformance(args, readme_text)
            finally:
                refresh_readme.ROOT = old_root
                refresh_readme.current_head_sha = old_head

        # SHA mismatch -> the artifact is treated as possibly stale, README wins.
        self.assertEqual(selected["passed"], 9)
        self.assertEqual(selected["total"], 10)
        self.assertIn("preserving README conformance metrics", stderr.getvalue())

    def test_emit_zero_drift_publishes_downward_reading(self):
        head = "0123456789abcdef0123456789abcdef01234567"
        with TemporaryDirectory() as temp_dir:
            root = pathlib.Path(temp_dir)
            detail_path = root / "scripts" / "emit" / "emit-detail.json"
            detail_path.parent.mkdir(parents=True)
            detail_path.write_text(json.dumps({
                "git_sha": head,
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
            old_head = refresh_readme.current_head_sha
            stderr = io.StringIO()
            try:
                refresh_readme.ROOT = root
                refresh_readme.current_head_sha = lambda: head
                with redirect_stderr(stderr):
                    selected = refresh_readme.load_emit(args, readme_text)
            finally:
                refresh_readme.ROOT = old_root
                refresh_readme.current_head_sha = old_head

        self.assertEqual(selected["jsPass"], 13094)
        self.assertEqual(selected["dtsPass"], 1606)
        self.assertIn("current HEAD", stderr.getvalue())


if __name__ == "__main__":
    unittest.main()
