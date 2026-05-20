#!/usr/bin/env python3
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("visited-clone-report.py")


def load_module():
    spec = importlib.util.spec_from_file_location("visited_clone_report", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class VisitedCloneReportTests(unittest.TestCase):
    def setUp(self):
        self.module = load_module()

    def write(self, root: Path, rel: str, text: str) -> None:
        path = root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def test_scan_reports_visited_clone_and_ignores_other_clones(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            self.write(
                root,
                "src/lib.rs",
                "\n".join(
                    [
                        "fn walk(visited: &mut Vec<u32>, names: Vec<String>) {",
                        "    let mut branch_visited = visited.clone();",
                        "    let other = names.clone();",
                        "    let visited_aliases = branch_visited.clone();",
                        "}",
                    ]
                )
                + "\n",
            )
            candidates = self.module.scan([root / "src"])

        self.assertEqual(
            [candidate.name for candidate in candidates],
            ["visited", "branch_visited"],
        )

    def test_scan_ignores_tests_and_comments(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            self.write(root, "src/lib.rs", "// let x = visited.clone();\n")
            self.write(root, "src/tests/ignored.rs", "let x = visited.clone();\n")
            candidates = self.module.scan([root / "src"])

        self.assertEqual(candidates, [])

    def test_cli_json_output_is_machine_readable(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            self.write(
                root,
                "src/lib.rs",
                "fn walk(visited_modules: Vec<u32>) { let x = visited_modules.clone(); }\n",
            )
            result = subprocess.run(
                ["python3", str(SCRIPT), "--root", str(root / "src"), "--json"],
                check=False,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        self.assertEqual(payload["summary"]["schema_version"], 1)
        self.assertEqual(payload["summary"]["total"], 1)
        self.assertEqual(payload["hits"][0]["name"], "visited_modules")


if __name__ == "__main__":
    unittest.main()
