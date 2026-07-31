"""Tests for check-test-file-reachability.py (#16013).

Two layers: synthetic-fixture tests that prove the detector actually fires
on an unreachable file and stays quiet on the reachable shapes the repo
uses (Cargo.toml `[[test]]`, same-crate `src/` `#[path]`, cross-crate `src/`
`#[path]`, and the >2000-line split-file `mod part_NN` pattern), then one
real-repo test that runs the detector against the actual `crates/` tree and
holds it to the checked-in baseline.
"""

import importlib.util
import pathlib
import shutil
import tempfile
import textwrap
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "ci" / "check-test-file-reachability.py"
BASELINE_PATH = ROOT / "scripts" / "ci" / "orphaned-test-files-baseline.txt"

spec = importlib.util.spec_from_file_location("check_test_file_reachability", SCRIPT)
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)


def _write(path: pathlib.Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(textwrap.dedent(text), encoding="utf-8")


class DetectorFixtureTests(unittest.TestCase):
    """Builds a synthetic `crates/` tree per test so each reachability shape
    is checked in isolation, independent of the real repo's current state."""

    def setUp(self):
        tmp = tempfile.mkdtemp(prefix="test-file-reachability-")
        self.addCleanup(shutil.rmtree, tmp, True)
        self.crates_dir = pathlib.Path(tmp) / "crates"
        orig_crates_dir = guard.CRATES_DIR
        guard.CRATES_DIR = self.crates_dir
        self.addCleanup(setattr, guard, "CRATES_DIR", orig_crates_dir)

    def _make_crate(self, name, autotests_false=True, extra_manifest=""):
        crate_dir = self.crates_dir / name
        manifest = f'[package]\nname = "{name}"\n'
        if autotests_false:
            manifest += "autotests = false\n"
        manifest += extra_manifest
        _write(crate_dir / "Cargo.toml", manifest)
        _write(crate_dir / "src" / "lib.rs", "")
        return crate_dir

    def test_unreachable_file_with_test_fn_is_flagged(self):
        self._make_crate("tsz-fixture-a")
        _write(
            self.crates_dir / "tsz-fixture-a" / "tests" / "orphan_tests.rs",
            """
            #[test]
            fn it_runs() {}
            """,
        )
        self.assertEqual(
            guard.find_orphaned_test_files(), ["tsz-fixture-a/tests/orphan_tests.rs"]
        )

    def test_file_registered_via_cargo_test_stanza_is_not_flagged(self):
        self._make_crate(
            "tsz-fixture-b",
            extra_manifest=(
                '[[test]]\nname = "wired_tests"\npath = "tests/wired_tests.rs"\n'
            ),
        )
        _write(
            self.crates_dir / "tsz-fixture-b" / "tests" / "wired_tests.rs",
            """
            #[test]
            fn it_runs() {}
            """,
        )
        self.assertEqual(guard.find_orphaned_test_files(), [])

    def test_file_included_via_same_crate_src_path_is_not_flagged(self):
        self._make_crate("tsz-fixture-c")
        _write(
            self.crates_dir / "tsz-fixture-c" / "src" / "lib.rs",
            """
            #[cfg(test)]
            #[path = "../tests/included_tests.rs"]
            mod included_tests;
            """,
        )
        _write(
            self.crates_dir / "tsz-fixture-c" / "tests" / "included_tests.rs",
            """
            #[test]
            fn it_runs() {}
            """,
        )
        self.assertEqual(guard.find_orphaned_test_files(), [])

    def test_file_included_via_cross_crate_src_path_is_not_flagged(self):
        # Mirrors crates/tsz-core/src/lib.rs reaching into
        # crates/tsz-checker/tests/ across a crate boundary.
        self._make_crate("tsz-fixture-target")
        _write(
            self.crates_dir / "tsz-fixture-target" / "tests" / "shared_tests.rs",
            """
            #[test]
            fn it_runs() {}
            """,
        )
        self._make_crate("tsz-fixture-other", autotests_false=False)
        _write(
            self.crates_dir / "tsz-fixture-other" / "src" / "lib.rs",
            """
            #[cfg(test)]
            #[path = "../../tsz-fixture-target/tests/shared_tests.rs"]
            mod shared_tests;
            """,
        )
        self.assertEqual(guard.find_orphaned_test_files(), [])

    def test_split_file_tests_only_in_a_mod_part_are_still_detected(self):
        # Mirrors crates/tsz-checker/tests/conditional_infer_tests.rs: the
        # root file has zero #[test] itself and only `mod`s in part files
        # that hold the real fns.
        self._make_crate("tsz-fixture-d")
        _write(
            self.crates_dir / "tsz-fixture-d" / "tests" / "split_tests.rs",
            """
            #[path = "split_tests/part_00.rs"]
            mod part_00;
            """,
        )
        _write(
            self.crates_dir
            / "tsz-fixture-d"
            / "tests"
            / "split_tests"
            / "part_00.rs",
            """
            #[test]
            fn it_runs() {}
            """,
        )
        self.assertEqual(
            guard.find_orphaned_test_files(), ["tsz-fixture-d/tests/split_tests.rs"]
        )

    def test_helper_file_with_no_test_fns_is_never_flagged(self):
        # Shared test-support modules (e.g. tests/test_support.rs) have zero
        # #[test] fns and are correctly unregistered as their own target.
        self._make_crate("tsz-fixture-e")
        _write(
            self.crates_dir / "tsz-fixture-e" / "tests" / "test_support.rs",
            "pub fn helper() {}\n",
        )
        self.assertEqual(guard.find_orphaned_test_files(), [])

    def test_crate_without_autotests_false_is_not_checked(self):
        # Cargo autodiscovers direct tests/*.rs files by default; only crates
        # that opt out with autotests = false need an explicit reach path.
        self._make_crate("tsz-fixture-f", autotests_false=False)
        _write(
            self.crates_dir / "tsz-fixture-f" / "tests" / "auto_discovered_tests.rs",
            """
            #[test]
            fn it_runs() {}
            """,
        )
        self.assertEqual(guard.find_orphaned_test_files(), [])


class MainCliTests(unittest.TestCase):
    def setUp(self):
        self.orig_find = guard.find_orphaned_test_files
        self.orig_baseline = guard.BASELINE_PATH
        self.addCleanup(setattr, guard, "find_orphaned_test_files", self.orig_find)
        self.addCleanup(setattr, guard, "BASELINE_PATH", self.orig_baseline)
        tmp = tempfile.mkdtemp(prefix="test-file-reachability-main-")
        self.addCleanup(shutil.rmtree, tmp, True)
        self.tmp_baseline = pathlib.Path(tmp) / "baseline.txt"
        guard.BASELINE_PATH = self.tmp_baseline

    def test_main_reports_new_orphans_not_in_baseline(self):
        self.tmp_baseline.write_text("# empty\n", encoding="utf-8")
        guard.find_orphaned_test_files = lambda: ["tsz-x/tests/new_orphan.rs"]
        self.assertEqual(guard.main(), 1)

    def test_main_is_clean_when_orphans_match_baseline_exactly(self):
        self.tmp_baseline.write_text(
            "tsz-x/tests/known_orphan.rs\n", encoding="utf-8"
        )
        guard.find_orphaned_test_files = lambda: ["tsz-x/tests/known_orphan.rs"]
        self.assertEqual(guard.main(), 0)

    def test_baseline_shrinking_below_current_orphans_stays_clean(self):
        # SHRINK is always allowed even when not exercised yet: a baseline
        # entry that no longer reproduces (file fixed but not pruned) must
        # not fail the gate.
        self.tmp_baseline.write_text(
            "tsz-x/tests/known_orphan.rs\ntsz-x/tests/already_fixed.rs\n",
            encoding="utf-8",
        )
        guard.find_orphaned_test_files = lambda: ["tsz-x/tests/known_orphan.rs"]
        self.assertEqual(guard.main(), 0)


class RealRepoReachabilityTests(unittest.TestCase):
    """The actual gate: run the detector against the real crates/ tree."""

    def test_current_orphans_are_exactly_the_checked_in_baseline(self):
        current = set(guard.find_orphaned_test_files())
        baseline = guard.parse_baseline(BASELINE_PATH.read_text(encoding="utf-8"))
        new_orphans = sorted(current - baseline)
        self.assertEqual(
            new_orphans,
            [],
            msg=(
                f"New orphaned test file(s) not in {BASELINE_PATH.name}: "
                f"{new_orphans}"
            ),
        )


if __name__ == "__main__":
    unittest.main()
