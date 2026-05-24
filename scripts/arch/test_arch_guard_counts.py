import pathlib
import tempfile
import unittest

from arch_guard_test_support import ROOT, load_arch_guard_module


class ArchGuardIndependentPipelineTests(unittest.TestCase):
    """Cover `INDEPENDENT_PIPELINE_CHECKS` + `scan_independent_pipelines`.

    Architecture health metric 4 anchor — workstream 3 exit criterion is
    "one blessed parse-bind-check path".  These tests pin the detection
    semantics so future contributors who refactor `scan_independent_pipelines`
    keep the invariants:

      - file with all three of `ParserState::new`, `BinderState::new`,
        `CheckerState::new` counts
      - file with two-of-three doesn't count
      - test files (`*_tests.rs`, files in `tests/`) are excluded
      - the pinned cap matches the live count
    """

    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _make_tree(self, files: dict[str, str]):
        """Materialize `files` into a temp directory; return the dir path."""
        tmp = tempfile.mkdtemp()
        root = pathlib.Path(tmp)
        for rel, content in files.items():
            full = root / rel
            full.parent.mkdir(parents=True, exist_ok=True)
            full.write_text(content, encoding="utf-8")
        return root

    def test_counts_files_with_all_three_constructors(self):
        root = self._make_tree(
            {
                "src/all_three.rs": (
                    "use tsz_parser::ParserState;\n"
                    "let mut p = ParserState::new(\"\".into(), \"\".into());\n"
                    "let mut b = BinderState::new();\n"
                    "let mut c = CheckerState::new();\n"
                ),
                "src/only_parser.rs": (
                    "let mut p = ParserState::new(\"\".into(), \"\".into());\n"
                ),
                "src/parser_and_binder.rs": (
                    "let p = ParserState::new(\"\".into(), \"\".into());\n"
                    "let b = BinderState::new();\n"
                ),
            }
        )
        # Cap at 0 — there's exactly 1 full-pipeline file, so 1 hit + summary.
        hits = self.arch_guard.scan_independent_pipelines([root], 0)
        # Each pipeline file gets its own line plus a final summary line.
        self.assertEqual(len(hits), 2, f"unexpected hits: {hits!r}")
        self.assertIn("all_three.rs", hits[0])
        self.assertIn("total independent parse-bind-check pipelines: 1", hits[1])

    def test_excludes_test_files(self):
        root = self._make_tree(
            {
                "src/foo_tests.rs": (
                    "let p = ParserState::new(\"\".into(), \"\".into());\n"
                    "let b = BinderState::new();\n"
                    "let c = CheckerState::new();\n"
                ),
                "tests/some_test.rs": (
                    "let p = ParserState::new(\"\".into(), \"\".into());\n"
                    "let b = BinderState::new();\n"
                    "let c = CheckerState::new();\n"
                ),
                "src/test_helpers.rs": (
                    "let p = ParserState::new(\"\".into(), \"\".into());\n"
                    "let b = BinderState::new();\n"
                    "let c = CheckerState::new();\n"
                ),
            }
        )
        # `is_test_file` excludes `*_tests.rs` and files starting with
        # `test_`. Tests under `tests/` are also excluded by the search-root
        # filter via iter_rs_files. Cap at 0 — should still pass.
        hits = self.arch_guard.scan_independent_pipelines([root], 0)
        # `tests/some_test.rs` may not be excluded depending on
        # `iter_rs_files` semantics, but `_tests.rs` and `test_*.rs` are
        # excluded by `is_test_file`.
        for hit in hits:
            self.assertNotIn("foo_tests.rs", hit)
            self.assertNotIn("test_helpers.rs", hit)

    def test_passes_when_at_or_under_cap(self):
        root = self._make_tree(
            {
                "src/a.rs": (
                    "let p = ParserState::new(\"\".into(), \"\".into());\n"
                    "let b = BinderState::new();\n"
                    "let c = CheckerState::new();\n"
                ),
                "src/b.rs": (
                    "let p = ParserState::new(\"\".into(), \"\".into());\n"
                    "let b = BinderState::new();\n"
                    "let c = CheckerState::new();\n"
                ),
            }
        )
        self.assertEqual(self.arch_guard.scan_independent_pipelines([root], 2), [])
        self.assertEqual(self.arch_guard.scan_independent_pipelines([root], 5), [])

    def test_check_is_registered(self):
        names = [entry[0] for entry in self.arch_guard.INDEPENDENT_PIPELINE_CHECKS]
        self.assertTrue(
            any("metric 4" in name for name in names),
            "Independent-pipeline guard is missing from INDEPENDENT_PIPELINE_CHECKS",
        )

    def test_real_pipelines_pass_at_pinned_cap(self):
        """The pinned cap must match the live count (no off-by-one)."""
        for entry in self.arch_guard.INDEPENDENT_PIPELINE_CHECKS:
            name, search_roots, max_pipelines = entry
            hits = self.arch_guard.scan_independent_pipelines(
                search_roots, max_pipelines
            )
            self.assertEqual(
                hits,
                [],
                f"{name}: cap is too tight — guard fires at the live count.",
            )


class ArchGuardSolverImportCountTests(unittest.TestCase):
    """Cover `SOLVER_IMPORT_COUNT_CHECKS` + `scan_solver_import_count`.

    Architecture health metric 7 anchor — workstream 3 ("Compiler Service
    Front Door") wants frontends and emitter/lowering crates to converge
    through one compiler service. These tests pin the detection semantics
    so future contributors who refactor `scan_solver_import_count` keep
    the invariants:

      - `use tsz_solver::...`, `pub use tsz_solver`, and
        `extern crate tsz_solver` are all flagged
      - test files (`*_tests.rs`, `test_*.rs`, files under `tests/` or
        `benches/`) are excluded
      - paths starting with the exclude prefixes (solver crate, checker
        crate) are skipped
      - comment-only lines are not counted
      - the pinned cap matches the live count
    """

    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _make_tree(self, files: dict[str, str]):
        """Materialize `files` into a temp directory; return the dir path."""
        tmp = tempfile.mkdtemp()
        root = pathlib.Path(tmp)
        for rel, content in files.items():
            full = root / rel
            full.parent.mkdir(parents=True, exist_ok=True)
            full.write_text(content, encoding="utf-8")
        return root

    def test_flags_use_pub_use_and_extern_crate_imports(self):
        root = self._make_tree(
            {
                "crates/tsz-cli/src/use_form.rs": (
                    "use tsz_solver::TypeId;\n"
                ),
                "crates/tsz-core/src/pub_use_form.rs": (
                    "pub use tsz_solver;\n"
                ),
                "crates/tsz-lsp/src/extern_form.rs": (
                    "extern crate tsz_solver;\n"
                ),
            }
        )
        # Cap at 0 — there are 3 importing files, so 3 hits + summary.
        hits = self.arch_guard.scan_solver_import_count([root], (), 0)
        self.assertEqual(len(hits), 4, f"unexpected hits: {hits!r}")
        self.assertIn("use_form.rs", hits[0])
        self.assertIn("pub_use_form.rs", hits[1])
        self.assertIn("extern_form.rs", hits[2])
        self.assertIn(
            "total direct tsz_solver imports outside solver/checker: 3",
            hits[3],
        )

    def test_excludes_test_files_and_test_dirs(self):
        root = self._make_tree(
            {
                "crates/tsz-cli/src/foo_tests.rs": "use tsz_solver::TypeId;\n",
                "crates/tsz-cli/src/test_helpers.rs": "use tsz_solver::TypeId;\n",
                "crates/tsz-cli/tests/integration.rs": (
                    "use tsz_solver::TypeId;\n"
                ),
                "crates/tsz-core/benches/bench.rs": (
                    "use tsz_solver::TypeId;\n"
                ),
            }
        )
        # All four files are test/bench files — should pass at cap=0.
        hits = self.arch_guard.scan_solver_import_count([root], (), 0)
        self.assertEqual(hits, [], f"unexpected hits: {hits!r}")

    def test_excludes_paths_under_exclude_prefixes(self):
        root = self._make_tree(
            {
                "crates/tsz-solver/src/internal.rs": (
                    "use tsz_solver::TypeId;\n"
                ),
                "crates/tsz-checker/src/query_boundaries/foo.rs": (
                    "use tsz_solver::TypeId;\n"
                ),
                "crates/tsz-checker/src/checker.rs": (
                    "use tsz_solver::TypeId;\n"
                ),
                "crates/tsz-cli/src/driver.rs": (
                    "use tsz_solver::TypeId;\n"
                ),
            }
        )
        exclude_prefixes = ("crates/tsz-solver/", "crates/tsz-checker/")
        # Only `tsz-cli/src/driver.rs` is in scope — 1 hit + summary at cap=0.
        hits = self.arch_guard.scan_solver_import_count([root], exclude_prefixes, 0)
        self.assertEqual(len(hits), 2, f"unexpected hits: {hits!r}")
        self.assertIn("driver.rs", hits[0])
        self.assertNotIn("internal.rs", hits[0])
        self.assertNotIn("query_boundaries", hits[0])
        self.assertNotIn("checker.rs", hits[0])

    def test_ignores_comment_only_lines(self):
        root = self._make_tree(
            {
                "crates/tsz-cli/src/commented.rs": (
                    "// use tsz_solver::TypeId;\n"
                    "// pub use tsz_solver;\n"
                ),
            }
        )
        hits = self.arch_guard.scan_solver_import_count([root], (), 0)
        self.assertEqual(hits, [], f"unexpected hits: {hits!r}")

    def test_passes_when_at_or_under_cap(self):
        root = self._make_tree(
            {
                "crates/tsz-cli/src/a.rs": "use tsz_solver::TypeId;\n",
                "crates/tsz-core/src/b.rs": "use tsz_solver::TypeId;\n",
            }
        )
        # Two importing files, cap=2 → exact match → no hits.
        self.assertEqual(self.arch_guard.scan_solver_import_count([root], (), 2), [])
        # Cap above live count → still no hits.
        self.assertEqual(self.arch_guard.scan_solver_import_count([root], (), 5), [])

    def test_check_is_registered(self):
        names = [entry[0] for entry in self.arch_guard.SOLVER_IMPORT_COUNT_CHECKS]
        self.assertTrue(
            any("metric 7" in name for name in names),
            "Solver-import-count guard is missing from SOLVER_IMPORT_COUNT_CHECKS",
        )

    def test_real_imports_pass_at_pinned_cap(self):
        """The pinned cap must match the live count (no off-by-one)."""
        for entry in self.arch_guard.SOLVER_IMPORT_COUNT_CHECKS:
            name, search_roots, exclude_path_prefixes, max_imports = entry
            hits = self.arch_guard.scan_solver_import_count(
                search_roots, exclude_path_prefixes, max_imports
            )
            self.assertEqual(
                hits,
                [],
                f"{name}: cap is too tight — guard fires at the live count.",
            )


class ArchGuardRootSolverComputationImportCountTests(unittest.TestCase):
    """Cover the #8204 ratchet for flat root solver computation APIs."""

    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _make_tree(self, files: dict[str, str]):
        tmp = tempfile.mkdtemp()
        root = pathlib.Path(tmp)
        for rel, content in files.items():
            full = root / rel
            full.parent.mkdir(parents=True, exist_ok=True)
            full.write_text(content, encoding="utf-8")
        return root

    def test_flags_direct_and_grouped_flat_computation_imports(self):
        root = self._make_tree(
            {
                "crates/tsz-lsp/src/member.rs": (
                    "let ty = tsz_solver::evaluate_type(interner, ty);\n"
                ),
                "crates/tsz-emitter/src/declaration.rs": (
                    "use tsz_solver::{TypeId, TypeSubstitution};\n"
                ),
            }
        )
        hits = self.arch_guard.scan_root_solver_computation_import_count(
            [root], (), 0
        )
        self.assertEqual(len(hits), 3, f"unexpected hits: {hits!r}")
        self.assertIn("declaration.rs:1", hits[0])
        self.assertIn("member.rs:1", hits[1])
        self.assertIn("total flat root solver computation API references", hits[2])

    def test_excludes_query_boundaries_tests_and_comment_lines(self):
        root = self._make_tree(
            {
                "crates/tsz-checker/src/query_boundaries/assignability.rs": (
                    "let checker = tsz_solver::CompatChecker::new(db);\n"
                ),
                "crates/tsz-lsp/src/foo_tests.rs": (
                    "let ty = tsz_solver::evaluate_type(interner, ty);\n"
                ),
                "crates/tsz-emitter/tests/declaration.rs": (
                    "use tsz_solver::TypeSubstitution;\n"
                ),
                "crates/tsz-cli/src/commented.rs": (
                    "// let ty = tsz_solver::evaluate_type(interner, ty);\n"
                ),
            }
        )
        hits = self.arch_guard.scan_root_solver_computation_import_count(
            [root], ("crates/tsz-checker/src/query_boundaries/",), 0
        )
        self.assertEqual(hits, [], f"unexpected hits: {hits!r}")

    def test_passes_when_at_or_under_cap(self):
        root = self._make_tree(
            {
                "crates/tsz-lsp/src/member.rs": (
                    "let ty = tsz_solver::evaluate_type(interner, ty);\n"
                ),
                "crates/tsz-emitter/src/declaration.rs": (
                    "let sub = tsz_solver::TypeSubstitution::new();\n"
                ),
            }
        )
        scan = self.arch_guard.scan_root_solver_computation_import_count
        self.assertEqual(scan([root], (), 2), [])
        self.assertEqual(scan([root], (), 3), [])

    def test_check_is_registered(self):
        names = [
            entry[0]
            for entry in self.arch_guard.ROOT_SOLVER_COMPUTATION_IMPORT_COUNT_CHECKS
        ]
        self.assertTrue(any("#8204" in name for name in names))

    def test_real_count_passes_at_pinned_cap(self):
        """The pinned cap must match the live count (no off-by-one)."""
        for entry in self.arch_guard.ROOT_SOLVER_COMPUTATION_IMPORT_COUNT_CHECKS:
            name, search_roots, exclude_path_prefixes, max_references = entry
            hits = self.arch_guard.scan_root_solver_computation_import_count(
                search_roots, exclude_path_prefixes, max_references
            )
            self.assertEqual(
                hits,
                [],
                f"{name}: cap is too tight — guard fires at the live count.",
            )

    def test_flags_root_solver_explicit_computation_reexports(self):
        root = self._make_tree(
            {
                "crates/tsz-solver/src/lib.rs": (
                    "pub use evaluation::evaluate::{evaluate_type, TypeEvaluator};\n"
                    "pub use operations::widening;\n"
                    "pub use diagnostics::DiagnosticArg;\n"
                    "pub mod computation {\n"
                    "    pub use crate::evaluation::evaluate::evaluate_type;\n"
                    "}\n"
                    "// pub use instantiation::instantiate::TypeSubstitution;\n"
                ),
            }
        )
        lib_rs = root / "crates" / "tsz-solver" / "src" / "lib.rs"
        hits = self.arch_guard.scan_solver_root_explicit_reexport_count(
            lib_rs,
            ("evaluation", "operations", "instantiation"),
            0,
        )
        self.assertEqual(len(hits), 4, f"unexpected hits: {hits!r}")
        self.assertIn("lib.rs:1 evaluate_type", hits[0])
        self.assertIn("lib.rs:1 TypeEvaluator", hits[1])
        self.assertIn("lib.rs:2 widening", hits[2])
        self.assertIn("total flat solver root explicit computation re-exports", hits[3])

    def test_root_solver_explicit_reexport_count_passes_at_cap(self):
        root = self._make_tree(
            {
                "crates/tsz-solver/src/lib.rs": (
                    "pub use evaluation::evaluate::{evaluate_type, TypeEvaluator};\n"
                    "pub use diagnostics::DiagnosticArg;\n"
                ),
            }
        )
        lib_rs = root / "crates" / "tsz-solver" / "src" / "lib.rs"
        scan = self.arch_guard.scan_solver_root_explicit_reexport_count
        prefixes = ("evaluation",)
        self.assertEqual(scan(lib_rs, prefixes, 2), [])
        self.assertEqual(scan(lib_rs, prefixes, 3), [])

    def test_root_solver_explicit_reexport_check_is_registered(self):
        names = [
            entry[0]
            for entry in self.arch_guard.ROOT_SOLVER_EXPLICIT_REEXPORT_COUNT_CHECKS
        ]
        self.assertTrue(any("#8204" in name for name in names))

    def test_root_solver_explicit_reexport_real_count_passes_at_pinned_cap(self):
        """The explicit export cap must match the live count."""
        for entry in self.arch_guard.ROOT_SOLVER_EXPLICIT_REEXPORT_COUNT_CHECKS:
            name, file_path, prefixes, max_reexports = entry
            hits = self.arch_guard.scan_solver_root_explicit_reexport_count(
                file_path, prefixes, max_reexports
            )
            self.assertEqual(
                hits,
                [],
                f"{name}: cap is too tight — guard fires at the live count.",
            )


class ArchGuardQueryBoundaryCommonReferenceTests(unittest.TestCase):
    """Cover the #8225 ratchet for broad query-boundary common callers."""

    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _make_tree(self, files: dict[str, str]):
        tmp = tempfile.mkdtemp()
        root = pathlib.Path(tmp)
        for rel, content in files.items():
            full = root / rel
            full.parent.mkdir(parents=True, exist_ok=True)
            full.write_text(content, encoding="utf-8")
        return root

    def test_flags_direct_common_references_outside_query_boundaries(self):
        root = self._make_tree(
            {
                "crates/tsz-checker/src/checkers/foo.rs": (
                    "let shape = crate::query_boundaries::common::object_shape_for_type(db, ty);\n"
                    "let lazy = query_boundaries::common::lazy_def_id(db, ty);\n"
                ),
            }
        )
        hits = self.arch_guard.scan_query_boundary_common_reference_count(
            [root], ("crates/tsz-checker/src/query_boundaries/",), 0
        )
        self.assertEqual(len(hits), 3, f"unexpected hits: {hits!r}")
        self.assertIn("foo.rs:1", hits[0])
        self.assertIn("foo.rs:2", hits[1])
        self.assertIn("total direct query_boundaries::common references", hits[2])

    def test_excludes_query_boundaries_tests_and_comment_lines(self):
        root = self._make_tree(
            {
                "crates/tsz-checker/src/query_boundaries/flow_analysis.rs": (
                    "let shape = crate::query_boundaries::common::object_shape_for_type(db, ty);\n"
                ),
                "crates/tsz-checker/src/foo_tests.rs": (
                    "let shape = crate::query_boundaries::common::object_shape_for_type(db, ty);\n"
                ),
                "crates/tsz-checker/tests/integration.rs": (
                    "let shape = crate::query_boundaries::common::object_shape_for_type(db, ty);\n"
                ),
                "crates/tsz-checker/src/commented.rs": (
                    "// let shape = crate::query_boundaries::common::object_shape_for_type(db, ty);\n"
                ),
            }
        )
        hits = self.arch_guard.scan_query_boundary_common_reference_count(
            [root], ("crates/tsz-checker/src/query_boundaries/",), 0
        )
        self.assertEqual(hits, [], f"unexpected hits: {hits!r}")

    def test_passes_when_at_or_under_cap(self):
        root = self._make_tree(
            {
                "crates/tsz-checker/src/checkers/a.rs": (
                    "let shape = crate::query_boundaries::common::object_shape_for_type(db, ty);\n"
                ),
                "crates/tsz-checker/src/checkers/b.rs": (
                    "let lazy = query_boundaries::common::lazy_def_id(db, ty);\n"
                ),
            }
        )
        scan = self.arch_guard.scan_query_boundary_common_reference_count
        self.assertEqual(scan([root], (), 2), [])
        self.assertEqual(scan([root], (), 3), [])

    def test_check_is_registered(self):
        names = [
            entry[0]
            for entry in self.arch_guard.QUERY_BOUNDARY_COMMON_REFERENCE_COUNT_CHECKS
        ]
        self.assertTrue(any("#8225" in name for name in names))

    def test_real_count_passes_at_pinned_cap(self):
        """The pinned cap must match the live count (no slack)."""
        for entry in self.arch_guard.QUERY_BOUNDARY_COMMON_REFERENCE_COUNT_CHECKS:
            name, search_roots, exclude_path_prefixes, max_references = entry
            hits = self.arch_guard.scan_query_boundary_common_reference_count(
                search_roots, exclude_path_prefixes, max_references
            )
            self.assertEqual(
                hits,
                [],
                f"{name}: cap is too tight — guard fires at the live count.",
            )
            self.assertNotEqual(
                self.arch_guard.scan_query_boundary_common_reference_count(
                    search_roots, exclude_path_prefixes, max_references - 1
                ),
                [],
                f"{name}: cap has slack and should be ratcheted to the live count.",
            )


class ArchGuardQueryBoundaryModuleAllowanceTests(unittest.TestCase):
    """Cover the #8225 ratchet for broad query-boundary module allowances."""

    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _make_file(self, content: str) -> pathlib.Path:
        tmp = tempfile.mkdtemp()
        path = pathlib.Path(tmp) / "crates/tsz-checker/src/query_boundaries/mod.rs"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")
        return path

    def test_flags_allowance_entries_above_cap(self):
        path = self._make_file(
            "#[allow(dead_code, clippy::missing_const_for_fn)]\n"
            "pub(crate) mod foo;\n"
            "#[allow(clippy::manual_map)]\n"
            "pub(crate) mod bar;\n"
        )

        hits = self.arch_guard.scan_query_boundary_module_allowance_count(path, 2)

        self.assertEqual(len(hits), 4, f"unexpected hits: {hits!r}")
        self.assertIn("dead_code", hits[0])
        self.assertIn("clippy::missing_const_for_fn", hits[1])
        self.assertIn("clippy::manual_map", hits[2])
        self.assertIn("module-level lint allowance entries", hits[3])

    def test_ignores_comment_lines_and_passes_at_cap(self):
        path = self._make_file(
            "// #[allow(dead_code, clippy::manual_map)]\n"
            "#[allow(dead_code)]\n"
            "pub(crate) mod foo;\n"
        )

        hits = self.arch_guard.scan_query_boundary_module_allowance_count(path, 1)

        self.assertEqual(hits, [], f"unexpected hits: {hits!r}")

    def test_check_is_registered(self):
        names = [
            entry[0]
            for entry in self.arch_guard.QUERY_BOUNDARY_MODULE_ALLOWANCE_COUNT_CHECKS
        ]
        self.assertTrue(any("#8225" in name for name in names))

    def test_real_count_passes_at_pinned_cap(self):
        for entry in self.arch_guard.QUERY_BOUNDARY_MODULE_ALLOWANCE_COUNT_CHECKS:
            name, file_path, max_allowances = entry
            hits = self.arch_guard.scan_query_boundary_module_allowance_count(
                file_path, max_allowances
            )
            self.assertEqual(
                hits,
                [],
                f"{name}: cap is too tight — guard fires at the live count.",
            )


class ArchGuardWorkspaceClippyAllowTests(unittest.TestCase):
    """Cover the #9446 ratchet for workspace-wide Clippy suppression attributes.

    The guard detects any `#[allow(clippy::...)]`, `#![allow(clippy::...)]`, or
    `#[expect(clippy::...)]` attribute line in Rust sources under `crates/`.
    Comment lines are excluded.  The cap starts at the current inventory and
    must decrease to zero as cleanup slices land.
    """

    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _make_tree(self, files: dict[str, str]) -> pathlib.Path:
        tmp = tempfile.mkdtemp()
        root = pathlib.Path(tmp)
        for rel, content in files.items():
            full = root / rel
            full.parent.mkdir(parents=True, exist_ok=True)
            full.write_text(content, encoding="utf-8")
        return root

    def test_flags_item_level_allow(self):
        root = self._make_tree(
            {"crates/tsz-foo/src/a.rs": "#[allow(clippy::too_many_arguments)]\n"}
        )
        hits = self.arch_guard.scan_workspace_clippy_allow_count([root / "crates"], 0)
        self.assertEqual(len(hits), 2)
        self.assertIn("clippy suppression #1", hits[0])
        self.assertIn("total Clippy suppression", hits[1])

    def test_flags_crate_level_allow(self):
        root = self._make_tree(
            {"crates/tsz-foo/src/lib.rs": "#![allow(clippy::missing_const_for_fn)]\n"}
        )
        hits = self.arch_guard.scan_workspace_clippy_allow_count([root / "crates"], 0)
        self.assertEqual(len(hits), 2)

    def test_flags_expect_variant(self):
        root = self._make_tree(
            {"crates/tsz-foo/src/a.rs": "#[expect(clippy::cast_sign_loss)]\n"}
        )
        hits = self.arch_guard.scan_workspace_clippy_allow_count([root / "crates"], 0)
        self.assertEqual(len(hits), 2)

    def test_flags_mixed_allow_with_clippy(self):
        root = self._make_tree(
            {
                "crates/tsz-foo/src/a.rs": (
                    "#[allow(dead_code, clippy::match_same_arms)]\n"
                )
            }
        )
        hits = self.arch_guard.scan_workspace_clippy_allow_count([root / "crates"], 0)
        self.assertEqual(len(hits), 2)

    def test_ignores_comment_lines(self):
        root = self._make_tree(
            {
                "crates/tsz-foo/src/a.rs": (
                    "// #[allow(clippy::too_many_arguments)]\n"
                    "fn foo() {}\n"
                )
            }
        )
        hits = self.arch_guard.scan_workspace_clippy_allow_count([root / "crates"], 0)
        self.assertEqual(hits, [])

    def test_ignores_non_clippy_allow(self):
        root = self._make_tree(
            {"crates/tsz-foo/src/a.rs": "#[allow(dead_code)]\n"}
        )
        hits = self.arch_guard.scan_workspace_clippy_allow_count([root / "crates"], 0)
        self.assertEqual(hits, [])

    def test_passes_when_at_or_under_cap(self):
        root = self._make_tree(
            {
                "crates/a/src/a.rs": "#[allow(clippy::too_many_arguments)]\n",
                "crates/b/src/b.rs": "#[allow(clippy::match_same_arms)]\n",
            }
        )
        self.assertEqual(
            self.arch_guard.scan_workspace_clippy_allow_count([root / "crates"], 2),
            [],
        )
        self.assertEqual(
            self.arch_guard.scan_workspace_clippy_allow_count([root / "crates"], 3),
            [],
        )

    def test_includes_test_files(self):
        root = self._make_tree(
            {
                "crates/tsz-foo/tests/foo_tests.rs": (
                    "#[allow(clippy::assertions_on_constants)]\n"
                )
            }
        )
        hits = self.arch_guard.scan_workspace_clippy_allow_count([root / "crates"], 0)
        self.assertEqual(len(hits), 2, f"expected 2 hits (1 match + summary): {hits!r}")

    def test_check_is_registered(self):
        names = [
            entry[0] for entry in self.arch_guard.WORKSPACE_CLIPPY_ALLOW_COUNT_CHECKS
        ]
        self.assertTrue(any("#9446" in name for name in names))

    def test_real_count_passes_at_pinned_cap(self):
        for entry in self.arch_guard.WORKSPACE_CLIPPY_ALLOW_COUNT_CHECKS:
            name, search_roots, max_count = entry
            hits = self.arch_guard.scan_workspace_clippy_allow_count(
                search_roots, max_count
            )
            self.assertEqual(
                hits,
                [],
                f"{name}: cap is too tight — guard fires at the live count.",
            )


class ArchGuardSnapshotRollbackTests(unittest.TestCase):
    """Cover `SNAPSHOT_ROLLBACK_FILE_COUNT_CHECKS` +
    `scan_snapshot_rollback_file_count`.

    Architecture health metric 5 anchor — workstream 4 ("Checker State /
    Speculation"). These tests pin the detection semantics so future
    contributors who refactor `scan_snapshot_rollback_file_count` keep the
    invariants:

      - broad `CheckerContext::rollback_*` methods are flagged
      - `DiagnosticSpeculationSnapshot` holder rollback methods are ignored
      - snapshot restorers (`restore_ts2454_state`,
        `restore_implicit_any_closures`) are flagged
      - `*guard.rollback(` SpeculationGuard calls are flagged
      - test files (`*_tests.rs`, `test_*.rs`, files under `tests/` or
        `benches/`) are excluded
      - paths starting with the exclude prefixes (speculation.rs) are skipped
      - comment-only lines are not counted
      - the pinned cap matches the live count
    """

    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _make_tree(self, files: dict[str, str]):
        """Materialize `files` into a temp directory; return the dir path."""
        tmp = tempfile.mkdtemp()
        root = pathlib.Path(tmp)
        for rel, content in files.items():
            full = root / rel
            full.parent.mkdir(parents=True, exist_ok=True)
            full.write_text(content, encoding="utf-8")
        return root

    def test_flags_rollback_full_and_diagnostics_methods(self):
        root = self._make_tree(
            {
                "crates/tsz-checker/src/a.rs": (
                    "self.ctx.rollback_full(&snap);\n"
                ),
                "crates/tsz-checker/src/b.rs": (
                    "ctx.rollback_diagnostics(&snap);\n"
                ),
                "crates/tsz-checker/src/c.rs": (
                    "ctx.rollback_diagnostics_filtered(&snap, |_| true);\n"
                ),
                "crates/tsz-checker/src/d.rs": (
                    "ctx.rollback_and_replace_diagnostics(&snap, vec![]);\n"
                ),
                "crates/tsz-checker/src/e.rs": (
                    "ctx.rollback_return_type(&snap);\n"
                ),
            }
        )
        # 5 files, cap=0 → 5 caller hits + 1 summary line.
        hits = self.arch_guard.scan_snapshot_rollback_file_count([root], (), 0)
        self.assertEqual(len(hits), 6, f"unexpected hits: {hits!r}")
        self.assertIn(
            "total snapshot-rollback caller files outside speculation.rs: 5",
            hits[5],
        )

    def test_flags_split_chain_diagnostics_methods(self):
        root = self._make_tree(
            {
                "crates/tsz-checker/src/split.rs": (
                    "self.ctx\n"
                    "    .rollback_diagnostics_filtered(&snap, |_| true);\n"
                ),
                "crates/tsz-checker/src/named_context.rs": (
                    "checker_context\n"
                    "    .rollback_diagnostics(&snap);\n"
                ),
            }
        )
        hits = self.arch_guard.scan_snapshot_rollback_file_count([root], (), 0)
        self.assertEqual(len(hits), 3, f"unexpected hits: {hits!r}")
        self.assertTrue(any("split.rs" in hit for hit in hits), hits)
        self.assertTrue(any("named_context.rs" in hit for hit in hits), hits)
        self.assertIn(
            "total snapshot-rollback caller files outside speculation.rs: 2",
            hits[2],
        )

    def test_flags_snapshot_restorers(self):
        root = self._make_tree(
            {
                "crates/tsz-checker/src/ts2454.rs": (
                    "ctx.restore_ts2454_state(&snap);\n"
                ),
                "crates/tsz-checker/src/implicit.rs": (
                    "ctx.restore_implicit_any_closures(&snap);\n"
                ),
            }
        )
        hits = self.arch_guard.scan_snapshot_rollback_file_count([root], (), 0)
        self.assertEqual(len(hits), 3, f"unexpected hits: {hits!r}")
        self.assertIn(
            "total snapshot-rollback caller files outside speculation.rs: 2",
            hits[2],
        )

    def test_flags_speculation_guard_rollback_calls(self):
        root = self._make_tree(
            {
                "crates/tsz-checker/src/plain.rs": (
                    "guard.rollback(&mut self.ctx);\n"
                ),
                "crates/tsz-checker/src/prefixed.rs": (
                    "method_diag_guard.rollback(&mut self.ctx);\n"
                ),
            }
        )
        hits = self.arch_guard.scan_snapshot_rollback_file_count([root], (), 0)
        self.assertEqual(len(hits), 3, f"unexpected hits: {hits!r}")
        self.assertIn(
            "total snapshot-rollback caller files outside speculation.rs: 2",
            hits[2],
        )

    def test_does_not_flag_unrelated_rollback_methods(self):
        """A bare `.rollback(` on a non-guard receiver must not be counted."""
        root = self._make_tree(
            {
                "crates/tsz-checker/src/unrelated.rs": (
                    "transaction.rollback();\n"
                    "db.rollback(&conn);\n"
                ),
            }
        )
        hits = self.arch_guard.scan_snapshot_rollback_file_count([root], (), 0)
        self.assertEqual(hits, [], f"unexpected hits: {hits!r}")

    def test_ignores_diagnostic_speculation_snapshot_holder_methods(self):
        root = self._make_tree(
            {
                "crates/tsz-checker/src/snapshot_holder.rs": (
                    "let snap = DiagnosticSpeculationSnapshot::new(&self.ctx);\n"
                    "snap.rollback(&mut self.ctx.diagnostic_state());\n"
                    "snap.rollback_filtered(&mut self.ctx.diagnostic_state(), |_| true);\n"
                    "snap.commit(&mut self.ctx.diagnostic_state());\n"
                ),
            }
        )
        hits = self.arch_guard.scan_snapshot_rollback_file_count([root], (), 0)
        self.assertEqual(hits, [], f"unexpected hits: {hits!r}")

    def test_excludes_test_files_and_test_dirs(self):
        root = self._make_tree(
            {
                "crates/tsz-checker/src/foo_tests.rs": (
                    "ctx.rollback_full(&snap);\n"
                ),
                "crates/tsz-checker/src/test_helpers.rs": (
                    "ctx.rollback_full(&snap);\n"
                ),
                "crates/tsz-checker/tests/integration.rs": (
                    "ctx.rollback_full(&snap);\n"
                ),
                "crates/tsz-checker/benches/bench.rs": (
                    "ctx.rollback_full(&snap);\n"
                ),
            }
        )
        hits = self.arch_guard.scan_snapshot_rollback_file_count([root], (), 0)
        self.assertEqual(hits, [], f"unexpected hits: {hits!r}")

    def test_excludes_paths_under_exclude_prefixes(self):
        root = self._make_tree(
            {
                "crates/tsz-checker/src/context/speculation.rs": (
                    "self.rollback_full(&snap);\n"
                ),
                "crates/tsz-checker/src/checker.rs": (
                    "ctx.rollback_full(&snap);\n"
                ),
            }
        )
        exclude_prefixes = ("crates/tsz-checker/src/context/speculation.rs",)
        hits = self.arch_guard.scan_snapshot_rollback_file_count(
            [root], exclude_prefixes, 0
        )
        # Only `checker.rs` is in scope — 1 hit + summary at cap=0.
        self.assertEqual(len(hits), 2, f"unexpected hits: {hits!r}")
        self.assertIn("checker.rs", hits[0])
        self.assertNotIn("speculation.rs", hits[0])

    def test_ignores_comment_only_lines(self):
        root = self._make_tree(
            {
                "crates/tsz-checker/src/commented.rs": (
                    "// ctx.rollback_full(&snap);\n"
                    "// guard.rollback(&mut self.ctx);\n"
                ),
            }
        )
        hits = self.arch_guard.scan_snapshot_rollback_file_count([root], (), 0)
        self.assertEqual(hits, [], f"unexpected hits: {hits!r}")

    def test_counts_each_file_at_most_once(self):
        """A file with many rollback calls still counts as 1 file."""
        root = self._make_tree(
            {
                "crates/tsz-checker/src/many.rs": (
                    "ctx.rollback_full(&a);\n"
                    "ctx.rollback_diagnostics(&b);\n"
                    "guard.rollback(&mut self.ctx);\n"
                ),
            }
        )
        hits = self.arch_guard.scan_snapshot_rollback_file_count([root], (), 0)
        self.assertEqual(len(hits), 2, f"unexpected hits: {hits!r}")
        self.assertIn(
            "total snapshot-rollback caller files outside speculation.rs: 1",
            hits[1],
        )

    def test_passes_when_at_or_under_cap(self):
        root = self._make_tree(
            {
                "crates/tsz-checker/src/a.rs": "ctx.rollback_full(&s);\n",
                "crates/tsz-checker/src/b.rs": "ctx.rollback_full(&s);\n",
            }
        )
        # Two files, cap=2 → exact match → no hits.
        self.assertEqual(
            self.arch_guard.scan_snapshot_rollback_file_count([root], (), 2),
            [],
        )
        # Cap above live count → still no hits.
        self.assertEqual(
            self.arch_guard.scan_snapshot_rollback_file_count([root], (), 5),
            [],
        )

    def test_check_is_registered(self):
        names = [
            entry[0]
            for entry in self.arch_guard.SNAPSHOT_ROLLBACK_FILE_COUNT_CHECKS
        ]
        self.assertTrue(
            any("metric 5" in name for name in names),
            "Snapshot-rollback guard is missing from "
            "SNAPSHOT_ROLLBACK_FILE_COUNT_CHECKS",
        )

    def test_real_callers_pass_at_pinned_cap(self):
        """The pinned cap must match the live count (no off-by-one)."""
        for entry in self.arch_guard.SNAPSHOT_ROLLBACK_FILE_COUNT_CHECKS:
            name, search_roots, exclude_path_prefixes, max_files = entry
            hits = self.arch_guard.scan_snapshot_rollback_file_count(
                search_roots, exclude_path_prefixes, max_files
            )
            self.assertEqual(
                hits,
                [],
                f"{name}: cap is too tight — guard fires at the live count.",
            )


class ArchGuardLspFeatureMethodCountTests(unittest.TestCase):
    """Cover `LSP_FEATURE_METHOD_COUNT_CHECKS` + `scan_lsp_feature_method_count`.

    Architecture health metric 7 anchor — workstream 6 ("LSP And WASM
    As Service Clients") wants LSP request handling to mostly map
    protocol inputs to service queries; the raw count of feature
    dispatch methods on `Project` makes drift visible.
    """

    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _make_file(self, content: str) -> pathlib.Path:
        tmp = tempfile.mkdtemp()
        path = pathlib.Path(tmp) / "features.rs"
        path.write_text(content, encoding="utf-8")
        return path

    def test_flags_each_dispatch_verb(self):
        # 7 dispatch verbs (get_, provide_, prepare_, handle_, on_,
        # find_, resolve_) — cap=0 should fire all 7 plus a summary.
        path = self._make_file(
            "impl Project {\n"
            "    pub fn get_hover(&self) {}\n"
            "    pub fn provide_inlay_hints(&self) {}\n"
            "    pub fn prepare_call_hierarchy(&self) {}\n"
            "    pub fn handle_completion(&self) {}\n"
            "    pub fn on_did_open(&self) {}\n"
            "    pub fn find_references(&self) {}\n"
            "    pub fn resolve_completion(&self) {}\n"
            "}\n"
        )
        hits = self.arch_guard.scan_lsp_feature_method_count(path, 0)
        # 7 method hits + 1 summary line.
        self.assertEqual(len(hits), 8, f"unexpected hits: {hits!r}")
        joined = "\n".join(hits[:-1])
        for name in (
            "get_hover",
            "provide_inlay_hints",
            "prepare_call_hierarchy",
            "handle_completion",
            "on_did_open",
            "find_references",
            "resolve_completion",
        ):
            self.assertIn(name, joined)
        self.assertIn(
            "total LSP feature-dispatch methods",
            hits[-1],
        )
        self.assertIn(": 7 ", hits[-1])

    def test_does_not_flag_non_dispatch_verbs(self):
        # `set_`, `with_`, `is_`, `has_` are not dispatch verbs and
        # must not be counted.
        path = self._make_file(
            "impl Project {\n"
            "    pub fn set_file(&mut self) {}\n"
            "    pub fn with_options(&self) {}\n"
            "    pub fn is_dirty(&self) -> bool { true }\n"
            "    pub fn has_diagnostics(&self) -> bool { true }\n"
            "}\n"
        )
        hits = self.arch_guard.scan_lsp_feature_method_count(path, 0)
        self.assertEqual(hits, [], f"unexpected hits: {hits!r}")

    def test_does_not_flag_top_level_or_nested_pub_fn(self):
        # The pattern requires leading whitespace, so top-level
        # `pub fn get_*` and free-function dispatchers don't get
        # counted; we also skip lines that start with `//`.
        path = self._make_file(
            "// pub fn get_in_comment_doc() {}\n"
            "/// pub fn get_hover() {} — example in doc comment\n"
            "pub fn get_top_level() {}\n"   # no leading indent
            "impl Project {\n"
            "    pub fn get_real_method(&self) {}\n"
            "}\n"
        )
        hits = self.arch_guard.scan_lsp_feature_method_count(path, 0)
        # Only the `impl`-indented one counts: 1 hit + 1 summary.
        self.assertEqual(len(hits), 2, f"unexpected hits: {hits!r}")
        self.assertIn("get_real_method", hits[0])

    def test_passes_when_at_cap(self):
        path = self._make_file(
            "impl Project {\n"
            "    pub fn get_a(&self) {}\n"
            "    pub fn get_b(&self) {}\n"
            "}\n"
        )
        # Cap exactly equal to live count → no hits.
        self.assertEqual(self.arch_guard.scan_lsp_feature_method_count(path, 2), [])
        # Cap above live count → still no hits.
        self.assertEqual(self.arch_guard.scan_lsp_feature_method_count(path, 5), [])

    def test_async_fn_is_flagged(self):
        path = self._make_file(
            "impl Project {\n"
            "    pub async fn get_async_thing(&self) {}\n"
            "}\n"
        )
        hits = self.arch_guard.scan_lsp_feature_method_count(path, 0)
        self.assertEqual(len(hits), 2)
        self.assertIn("get_async_thing", hits[0])

    def test_check_is_registered(self):
        names = [
            entry[0] for entry in self.arch_guard.LSP_FEATURE_METHOD_COUNT_CHECKS
        ]
        self.assertTrue(
            any("metric 7" in name for name in names),
            "LSP feature-method-count guard missing from "
            "LSP_FEATURE_METHOD_COUNT_CHECKS",
        )

    def test_real_count_passes_at_pinned_cap(self):
        """The pinned cap must match the live count (no off-by-one)."""
        for entry in self.arch_guard.LSP_FEATURE_METHOD_COUNT_CHECKS:
            name, file_path, max_methods = entry
            hits = self.arch_guard.scan_lsp_feature_method_count(
                file_path, max_methods
            )
            self.assertEqual(
                hits,
                [],
                f"{name}: cap is too tight — guard fires at the live count.",
            )


class ArchGuardSpeculationGuardNameTests(unittest.TestCase):
    """Pin architecture health metric 6 ("Speculation APIs with surprising
    non-RAII behavior"). Verifies `scan_speculation_guard_struct_count`."""

    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _write(self, tmp: pathlib.Path, name: str, contents: str) -> pathlib.Path:
        path = tmp / name
        path.write_text(contents, encoding="utf-8")
        return path

    def test_no_guard_struct_passes_at_cap_zero(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = pathlib.Path(tmp)
            path = self._write(
                tmp,
                "speculation.rs",
                "pub(crate) struct DiagnosticSpeculationSnapshot { snapshot: u32 }\n",
            )
            self.assertEqual(
                self.arch_guard.scan_speculation_guard_struct_count(path, 0),
                [],
            )

    def test_one_guard_struct_fires_at_cap_zero(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = pathlib.Path(tmp)
            path = self._write(
                tmp,
                "speculation.rs",
                "pub(crate) struct DiagnosticSpeculationGuard { snapshot: u32 }\n",
            )
            hits = self.arch_guard.scan_speculation_guard_struct_count(path, 0)
            self.assertEqual(len(hits), 2)
            self.assertIn("DiagnosticSpeculationGuard", hits[0])

    def test_doc_comment_guard_reference_does_not_match(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = pathlib.Path(tmp)
            path = self._write(
                tmp,
                "speculation.rs",
                "/// Replaces the legacy `SpeculationGuard` struct.\n"
                "pub(crate) struct DiagnosticSpeculationSnapshot {}\n",
            )
            self.assertEqual(
                self.arch_guard.scan_speculation_guard_struct_count(path, 0),
                [],
            )

    def test_pub_struct_guard_matches_too(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = pathlib.Path(tmp)
            path = self._write(
                tmp,
                "speculation.rs",
                "pub struct OuterGuard { inner: u32 }\n",
            )
            hits = self.arch_guard.scan_speculation_guard_struct_count(path, 0)
            self.assertTrue(any("OuterGuard" in h for h in hits))

    def test_check_is_registered(self):
        names = [
            entry[0] for entry in self.arch_guard.SPECULATION_GUARD_NAME_CHECKS
        ]
        self.assertTrue(
            any("metric 6" in name for name in names),
            "Speculation guard-name check missing from "
            "SPECULATION_GUARD_NAME_CHECKS",
        )

    def test_real_speculation_file_passes_at_pinned_cap(self):
        """The live speculation.rs must satisfy the pinned cap of 0
        `…Guard` structs (PR #1213 already renamed the only offender)."""
        for entry in self.arch_guard.SPECULATION_GUARD_NAME_CHECKS:
            name, file_path, max_guard_count = entry
            hits = self.arch_guard.scan_speculation_guard_struct_count(
                file_path, max_guard_count
            )
            self.assertEqual(
                hits,
                [],
                f"{name}: cap is too tight — guard fires at the live count.",
            )




if __name__ == "__main__":
    unittest.main()
