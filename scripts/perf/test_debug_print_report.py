#!/usr/bin/env python3
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("debug-print-report.py")


def load_module():
    spec = importlib.util.spec_from_file_location("debug_print_report", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class DebugPrintReportTests(unittest.TestCase):
    def setUp(self):
        self.report = load_module()

    def write_file(self, root: Path, rel: str, text: str) -> Path:
        path = root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        return path

    def test_scans_compiler_sources_and_ignores_tests_and_comments(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            self.write_file(
                root,
                "crates/tsz-checker/src/lib.rs",
                "\n".join(
                    [
                        "pub fn report() {",
                        "    println!(\"debug\");",
                        "    let url = \"https://example.test//not-comment\";",
                        "    let rendered = \"dbg!(not_a_macro)\";",
                        "    dbg!(url); // real hit before the comment",
                        "    // eprintln!(\"comment\");",
                        "    //! println!(\"doc comment\");",
                        "    /* println!(\"block comment\"); */",
                        "}",
                    ]
                ),
            )
            self.write_file(
                root,
                "crates/tsz-checker/src/tests/debug.rs",
                "pub fn test_probe() { println!(\"test output is out of scope\"); }\n",
            )

            hits = self.report.scan(root, ("crates/tsz-checker/src",))

        self.assertEqual([hit.macro for hit in hits], ["println!", "dbg!"])
        self.assertEqual(hits[0].path, "crates/tsz-checker/src/lib.rs")

    def test_default_scan_dirs_exclude_cli_user_facing_output(self):
        self.assertIn("crates/tsz-checker/src", self.report.DEFAULT_SCAN_DIRS)
        self.assertIn("crates/tsz-core/src", self.report.DEFAULT_SCAN_DIRS)
        self.assertNotIn("crates/tsz-cli/src", self.report.DEFAULT_SCAN_DIRS)

    def test_json_cli_reports_summary_and_hits(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            self.write_file(
                root,
                "crates/tsz-core/src/module_resolver/mod.rs",
                "pub fn trace() { eprintln!(\"resolver trace\"); }\n",
            )
            result = subprocess.run(
                [
                    "python3",
                    str(SCRIPT),
                    "--root",
                    str(root),
                    "--scan-dir",
                    "crates/tsz-core/src",
                    "--json",
                ],
                check=False,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        self.assertEqual(payload["summary"]["total"], 1)
        self.assertEqual(payload["summary"]["by_macro"], {"eprintln!": 1})
        self.assertEqual(payload["hits"][0]["line"], 1)


if __name__ == "__main__":
    unittest.main()
