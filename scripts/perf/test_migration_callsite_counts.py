#!/usr/bin/env python3
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("migration_callsite_counts.py")


def load_module():
    spec = importlib.util.spec_from_file_location("migration_callsite_counts", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class MigrationCallsiteCountsTests(unittest.TestCase):
    def setUp(self):
        self.module = load_module()

    def write(self, root: Path, rel: str, text: str) -> None:
        path = root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def test_scan_counts_each_migration_callsite(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            self.write(
                root,
                "src/a.rs",
                "\n".join(
                    [
                        "CheckerState::with_parent_cache(parent);",
                        "CheckerState::with_parent_cache_attributed(parent, reason);",
                        "ctx.copy_symbol_file_targets_to(&mut child);",
                        "ctx.copy_symbol_file_targets_to_attributed(&mut child, reason);",
                        "pub fn with_parent_cache(parent: Parent) -> Self { todo!() }",
                        "// CheckerState::with_parent_cache(parent);",
                    ]
                )
                + "\n",
            )
            self.write(
                root,
                "src/tests/ignored.rs",
                "CheckerState::with_parent_cache(parent);\n",
            )
            summary = self.module.summarize(self.module.scan([root / "src"]))

        self.assertEqual(
            summary["counts"],
            {
                "with_parent_cache": 1,
                "with_parent_cache_attributed": 1,
                "copy_symbol_file_targets_to": 1,
                "copy_symbol_file_targets_to_attributed": 1,
            },
        )
        self.assertEqual(
            summary["files"]["a.rs"],
            {
                "with_parent_cache": 1,
                "with_parent_cache_attributed": 1,
                "copy_symbol_file_targets_to": 1,
                "copy_symbol_file_targets_to_attributed": 1,
            },
        )

    def test_scan_ignores_with_parent_cache_wrapper_delegate(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            self.write(
                root,
                "src/state/state.rs",
                "\n".join(
                    [
                        "pub fn with_parent_cache_attributed(parent: Parent) -> Self {",
                        "    CheckerState {",
                        "        ctx: CheckerContext::with_parent_cache(parent),",
                        "    }",
                        "}",
                    ]
                )
                + "\n",
            )
            self.write(
                root,
                "src/types/type_node_query_members.rs",
                "let child = CheckerContext::with_parent_cache(parent);\n",
            )
            summary = self.module.summarize(self.module.scan([root / "src"]))

        self.assertEqual(summary["counts"]["with_parent_cache"], 1)
        self.assertNotIn("state/state.rs", summary["files"])
        self.assertEqual(
            summary["files"]["types/type_node_query_members.rs"]["with_parent_cache"],
            1,
        )

    def test_scan_ignores_context_with_parent_cache_wrapper_delegate(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            self.write(
                root,
                "src/context/constructors.rs",
                "\n".join(
                    [
                        "pub fn with_parent_cache_attributed(parent: Parent) -> Self {",
                        "    Self::with_parent_cache(parent)",
                        "}",
                    ]
                )
                + "\n",
            )
            self.write(
                root,
                "src/types/type_node_query_members.rs",
                "let child = CheckerContext::with_parent_cache(parent);\n",
            )
            summary = self.module.summarize(self.module.scan([root / "src"]))

        self.assertEqual(summary["counts"]["with_parent_cache"], 1)
        self.assertNotIn("context/constructors.rs", summary["files"])
        self.assertEqual(
            summary["files"]["types/type_node_query_members.rs"]["with_parent_cache"],
            1,
        )

    def test_cli_json_output_is_machine_readable(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            self.write(root, "src/a.rs", "CheckerState::with_parent_cache(parent);\n")
            result = subprocess.run(
                ["python3", str(SCRIPT), "--root", str(root / "src"), "--json"],
                check=False,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        self.assertEqual(payload["schema_version"], 1)
        self.assertEqual(payload["counts"]["with_parent_cache"], 1)


if __name__ == "__main__":
    unittest.main()
