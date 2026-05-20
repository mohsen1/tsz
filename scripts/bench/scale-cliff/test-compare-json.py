#!/usr/bin/env python3
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).with_name("compare-json.py")


def load_module():
    spec = importlib.util.spec_from_file_location("scale_cliff_compare_json", SCRIPT_PATH)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


def artifact(rows):
    return {
        "schema_version": 1,
        "generated_at": "2026-05-20T00:00:00+00:00",
        "rows": rows,
    }


def row(fixture, checkers, overlay, delegations, compute, status="ok"):
    return {
        "fixture": fixture,
        "files": 10,
        "ratio_checkers_per_file": checkers,
        "ratio_overlay_per_file": overlay,
        "ratio_delegations_per_file": delegations,
        "ratio_compute_per_file": compute,
        "status": status,
        "exit_code": 0 if status == "ok" else 1,
    }


class ScaleCliffCompareJsonTests(unittest.TestCase):
    def setUp(self):
        self.module = load_module()

    def test_classifies_ratio_changes_by_threshold(self):
        previous = artifact(
            [
                row("monorepo-001", 1.0, 100.0, 2.0, 5.0),
                row("monorepo-002", 1.0, 100.0, 2.0, 5.0),
            ]
        )
        current = artifact(
            [
                row("monorepo-001", 1.15, 95.0, 2.04, 3.0),
                row("monorepo-002", 0.80, 130.0, 2.0, 5.0),
            ]
        )

        report = self.module.build_report(
            previous,
            current,
            previous_path=Path("previous.json"),
            current_path=Path("current.json"),
            generated_at="2026-05-20T01:00:00+00:00",
            threshold=0.10,
        )

        by_key = {
            (change["fixture"], change["field"]): change
            for change in report["changes"]
        }
        self.assertEqual(
            by_key[("monorepo-001", "ratio_checkers_per_file")]["status"],
            "regression",
        )
        self.assertEqual(
            by_key[("monorepo-002", "ratio_checkers_per_file")]["status"],
            "improvement",
        )
        self.assertEqual(
            by_key[("monorepo-001", "ratio_delegations_per_file")]["status"],
            "stable",
        )
        self.assertEqual(
            by_key[("monorepo-002", "ratio_overlay_per_file")]["relative_delta"],
            0.3,
        )
        self.assertEqual(report["totals"]["regressions"], 2)
        self.assertEqual(report["totals"]["improvements"], 2)
        self.assertEqual(report["totals"]["stable"], 4)

    def test_cli_writes_json_and_markdown_without_failing_by_default(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            previous_path = root / "previous.json"
            current_path = root / "current.json"
            json_path = root / "compare.json"
            markdown_path = root / "compare.md"
            previous_path.write_text(
                json.dumps(artifact([row("monorepo-001", 1.0, 100.0, 2.0, 5.0)])),
                encoding="utf-8",
            )
            current_path.write_text(
                json.dumps(artifact([row("monorepo-001", 1.2, 90.0, 2.0, 5.0)])),
                encoding="utf-8",
            )

            rc = self.module.main(
                [
                    str(previous_path),
                    str(current_path),
                    "--json-file",
                    str(json_path),
                    "--markdown-file",
                    str(markdown_path),
                    "--generated-at",
                    "2026-05-20T01:00:00+00:00",
                ]
            )

            self.assertEqual(rc, 0)
            report = json.loads(json_path.read_text(encoding="utf-8"))
            self.assertEqual(report["totals"]["regressions"], 1)
            markdown = markdown_path.read_text(encoding="utf-8")
            self.assertIn("Scale-Cliff Ratio Comparison", markdown)
            self.assertIn("monorepo-001", markdown)
            self.assertIn("ratio_checkers_per_file", markdown)

    def test_reports_new_and_missing_fixture_ratios(self):
        previous = artifact([row("monorepo-001", 1.0, 100.0, 2.0, 5.0)])
        current = artifact([row("monorepo-002", 1.0, 100.0, 2.0, 5.0)])

        report = self.module.build_report(
            previous,
            current,
            previous_path=Path("previous.json"),
            current_path=Path("current.json"),
            generated_at="2026-05-20T01:00:00+00:00",
            threshold=0.10,
        )

        self.assertEqual(report["totals"]["missing"], 4)
        self.assertEqual(report["totals"]["new"], 4)
        by_fixture = {
            change["fixture"]: change["status"]
            for change in report["changes"]
            if change["field"] == "ratio_checkers_per_file"
        }
        self.assertEqual(by_fixture["monorepo-001"], "missing")
        self.assertEqual(by_fixture["monorepo-002"], "new")

    def test_missing_ratio_in_both_artifacts_is_stable(self):
        previous = artifact([{"fixture": "monorepo-001"}])
        current = artifact([{"fixture": "monorepo-001"}])

        report = self.module.build_report(
            previous,
            current,
            previous_path=Path("previous.json"),
            current_path=Path("current.json"),
            generated_at="2026-05-20T01:00:00+00:00",
            threshold=0.10,
        )

        self.assertEqual(report["totals"]["new"], 0)
        self.assertEqual(report["totals"]["missing"], 0)
        self.assertEqual(report["totals"]["stable"], 4)
        self.assertTrue(
            all(change["status"] == "stable" for change in report["changes"])
        )

    def test_cli_can_fail_on_regression(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            previous_path = root / "previous.json"
            current_path = root / "current.json"
            json_path = root / "compare.json"
            previous_path.write_text(
                json.dumps(artifact([row("monorepo-001", 1.0, 100.0, 2.0, 5.0)])),
                encoding="utf-8",
            )
            current_path.write_text(
                json.dumps(artifact([row("monorepo-001", 1.2, 100.0, 2.0, 5.0)])),
                encoding="utf-8",
            )

            rc = self.module.main(
                [
                    str(previous_path),
                    str(current_path),
                    "--json-file",
                    str(json_path),
                    "--fail-on-regression",
                ]
            )

            self.assertEqual(rc, 1)


if __name__ == "__main__":
    unittest.main()
