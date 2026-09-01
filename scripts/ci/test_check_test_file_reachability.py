#!/usr/bin/env python3
"""Synthetic and repository contracts for active test-root reachability."""

from __future__ import annotations

from contextlib import redirect_stderr, redirect_stdout
import importlib.util
import io
from pathlib import Path
import shutil
import sys
import tempfile
import textwrap
import unittest


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "ci" / "check-test-file-reachability.py"

spec = importlib.util.spec_from_file_location("check_test_file_reachability", SCRIPT)
guard = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = guard
spec.loader.exec_module(guard)


def _write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(textwrap.dedent(text), encoding="utf-8")


class DetectorFixtureTests(unittest.TestCase):
    def setUp(self) -> None:
        temp_dir = tempfile.mkdtemp(prefix="active-test-reachability-")
        self.addCleanup(shutil.rmtree, temp_dir, True)
        self.crates_dir = Path(temp_dir) / "crates"
        self.original_crates_dir = guard.CRATES_DIR
        guard.CRATES_DIR = self.crates_dir
        self.addCleanup(setattr, guard, "CRATES_DIR", self.original_crates_dir)
        self.workspace_members = []
        self._write_workspace_manifest()

    def _write_workspace_manifest(self) -> None:
        members = ",\n".join(
            f'    "crates/{name}"' for name in self.workspace_members
        )
        _write(
            self.crates_dir.parent / "Cargo.toml",
            f"""
            [workspace]
            members = [
            {members}
            ]
            """,
        )

    def make_active_crate(
        self,
        name: str = "tsz-fixture",
        *,
        autotests: bool = False,
        active_root: str = "rewrite-tests",
    ) -> Path:
        crate_dir = self.crates_dir / name
        _write(
            crate_dir / "Cargo.toml",
            f"""
            [package]
            name = "{name}"
            version = "0.0.0"
            autotests = {str(autotests).lower()}

            [[test]]
            name = "active_root_anchor"
            path = "{active_root}/root.rs"
            """,
        )
        _write(crate_dir / "src" / "lib.rs", "")
        _write(crate_dir / active_root / "root.rs", "pub fn helper() {}\n")
        self.workspace_members.append(name)
        self._write_workspace_manifest()
        return crate_dir

    def add_cargo_test(self, crate_dir: Path, name: str, path: str) -> None:
        with (crate_dir / "Cargo.toml").open("a", encoding="utf-8") as manifest:
            manifest.write(
                textwrap.dedent(
                    f"""

                    [[test]]
                    name = "{name}"
                    path = "{path}"
                    """
                )
            )

    def test_unregistered_active_test_bearing_file_fails(self) -> None:
        crate_dir = self.make_active_crate()
        _write(
            crate_dir / "rewrite-tests" / "orphan.rs",
            "#[test]\nfn silently_never_runs() {}\n",
        )
        self.assertEqual(
            guard.find_unreachable_active_test_files(),
            ["crates/tsz-fixture/rewrite-tests/orphan.rs"],
        )

    def test_cargo_test_target_is_reachable(self) -> None:
        crate_dir = self.make_active_crate()
        self.add_cargo_test(crate_dir, "wired", "rewrite-tests/wired.rs")
        _write(
            crate_dir / "rewrite-tests" / "wired.rs",
            "#[test]\nfn runs() {}\n",
        )
        self.assertEqual(guard.find_unreachable_active_test_files(), [])

    def test_source_path_module_is_reachable(self) -> None:
        crate_dir = self.make_active_crate()
        _write(
            crate_dir / "src" / "lib.rs",
            """
            #[cfg(test)]
            #[path = "../rewrite-tests/unit.rs"]
            mod tests;
            """,
        )
        _write(
            crate_dir / "rewrite-tests" / "unit.rs",
            "#[test]\nfn runs_in_lib_harness() {}\n",
        )
        self.assertEqual(guard.find_unreachable_active_test_files(), [])

    def test_nested_path_modules_are_followed_recursively(self) -> None:
        crate_dir = self.make_active_crate()
        self.add_cargo_test(crate_dir, "root", "rewrite-tests/root.rs")
        _write(
            crate_dir / "rewrite-tests" / "root.rs",
            """
            #[path = "parts/first.rs"]
            mod first;
            """,
        )
        _write(
            crate_dir / "rewrite-tests" / "parts" / "first.rs",
            """
            #[path = "second.rs"]
            mod second;
            """,
        )
        _write(
            crate_dir / "rewrite-tests" / "parts" / "second.rs",
            "#[test]\nfn nested_test_runs() {}\n",
        )
        self.assertEqual(guard.find_unreachable_active_test_files(), [])

    def test_nested_conventional_modules_are_followed_recursively(self) -> None:
        crate_dir = self.make_active_crate()
        self.add_cargo_test(crate_dir, "root", "rewrite-tests/root.rs")
        _write(crate_dir / "rewrite-tests" / "root.rs", "mod parts;\n")
        _write(
            crate_dir / "rewrite-tests" / "parts.rs",
            "mod nested;\n",
        )
        _write(
            crate_dir / "rewrite-tests" / "parts" / "nested.rs",
            "#[test]\nfn nested_test_runs() {}\n",
        )
        self.assertEqual(guard.find_unreachable_active_test_files(), [])

    def test_unregistered_helper_root_does_not_make_child_reachable(self) -> None:
        crate_dir = self.make_active_crate()
        _write(
            crate_dir / "rewrite-tests" / "unwired.rs",
            """
            #[path = "unwired/part.rs"]
            mod part;
            """,
        )
        _write(
            crate_dir / "rewrite-tests" / "unwired" / "part.rs",
            "#[test]\nfn silently_never_runs() {}\n",
        )
        self.assertEqual(
            guard.find_unreachable_active_test_files(),
            ["crates/tsz-fixture/rewrite-tests/unwired/part.rs"],
        )

    def test_helper_only_file_may_remain_unregistered(self) -> None:
        crate_dir = self.make_active_crate()
        _write(
            crate_dir / "rewrite-tests" / "support.rs",
            "pub fn fixture_source() -> &'static str { \"let x = 1;\" }\n",
        )
        self.assertEqual(guard.find_unreachable_active_test_files(), [])

    def test_retained_legacy_test_tree_is_out_of_scope(self) -> None:
        crate_dir = self.make_active_crate()
        _write(
            crate_dir / "tests" / "legacy_orphan.rs",
            "#[test]\nfn intentionally_disabled_porting_case() {}\n",
        )
        self.assertEqual(guard.find_unreachable_active_test_files(), [])

    def test_active_conformance_tests_and_source_path_modules_are_reachable(self) -> None:
        crate_dir = self.make_active_crate(
            name="conformance", active_root="tests"
        )
        self.add_cargo_test(crate_dir, "canonical", "tests/canonical.rs")
        _write(
            crate_dir / "tests" / "canonical.rs",
            "#[test]\nfn canonical_contract_runs() {}\n",
        )
        _write(
            crate_dir / "src" / "lib.rs",
            """
            #[cfg(test)]
            #[path = "../tests/unit.rs"]
            mod unit;
            """,
        )
        _write(
            crate_dir / "tests" / "unit.rs",
            "#[test]\nfn source_path_unit_runs() {}\n",
        )
        self.assertEqual(guard.find_unreachable_active_test_files(), [])

    def test_retired_no_manifest_crate_is_out_of_scope(self) -> None:
        self.make_active_crate()
        _write(
            self.crates_dir / "retired-compiler" / "tests" / "orphan.rs",
            "#[test]\nfn archived_only() {}\n",
        )
        self.assertEqual(guard.find_unreachable_active_test_files(), [])

    def test_commented_and_stringified_test_attributes_are_not_tests(self) -> None:
        crate_dir = self.make_active_crate()
        _write(
            crate_dir / "rewrite-tests" / "not_a_test.rs",
            r'''
            // #[test]
            // fn commented_out() {}
            const SOURCE: &str = r#"
            #[test]
            fn fixture_text_only() {}
            "#;
            ''',
        )
        self.assertEqual(guard.find_unreachable_active_test_files(), [])

    def test_workspace_crate_with_cargo_autotest_discovery_is_not_checked(self) -> None:
        self.make_active_crate(autotests=True)
        self.assertEqual(guard.find_unreachable_active_test_files(), [])


class MainCliTests(unittest.TestCase):
    def setUp(self) -> None:
        self.original_find = guard.find_unreachable_active_test_files
        self.addCleanup(
            setattr,
            guard,
            "find_unreachable_active_test_files",
            self.original_find,
        )

    def test_main_rejects_unreachable_active_tests_without_a_baseline(self) -> None:
        guard.find_unreachable_active_test_files = lambda: [
            "crates/tsz-core/rewrite-tests/orphan.rs"
        ]
        stderr = io.StringIO()
        with redirect_stderr(stderr):
            self.assertEqual(guard.main(), 1)
        self.assertIn("No orphan baseline is allowed", stderr.getvalue())

    def test_main_accepts_an_empty_offender_set(self) -> None:
        guard.find_unreachable_active_test_files = lambda: []
        stdout = io.StringIO()
        with redirect_stdout(stdout):
            self.assertEqual(guard.main(), 0)
        self.assertIn("0 unreachable tests", stdout.getvalue())


class RealRepositoryTests(unittest.TestCase):
    def test_current_active_rewrite_tests_are_all_reachable(self) -> None:
        self.assertEqual(guard.find_unreachable_active_test_files(), [])

    def test_guard_has_no_legacy_baseline_or_legacy_test_root(self) -> None:
        source = SCRIPT.read_text(encoding="utf-8")
        self.assertNotIn("orphaned-test-files-baseline", source)
        active_roots = {
            root.relative_to(ROOT).as_posix()
            for crate_dir in guard.active_crate_dirs()
            for root in guard.active_test_dirs(crate_dir)
        }
        self.assertEqual(
            active_roots,
            {
                "crates/conformance/tests",
                "crates/tsz-cli/rewrite-tests",
                "crates/tsz-core/rewrite-tests",
            },
        )
        self.assertNotIn("crates/tsz-checker/tests", active_roots)


if __name__ == "__main__":
    unittest.main()
