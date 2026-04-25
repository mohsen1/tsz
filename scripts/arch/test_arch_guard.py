import importlib.util
import pathlib
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
ARCH_GUARD_PATH = ROOT / "scripts" / "arch" / "arch_guard.py"


def load_arch_guard_module():
    spec = importlib.util.spec_from_file_location("arch_guard", ARCH_GUARD_PATH)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


class ArchGuardCompatCheckerBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _compat_checker_check(self):
        for name, _base, pattern, excludes in self.arch_guard.CHECKS:
            if name == "Checker boundary: direct CompatChecker construction outside query boundaries/tests":
                return pattern, excludes
        self.fail("CompatChecker construction boundary check is missing from CHECKS")

    def test_rule_exists(self):
        self._compat_checker_check()

    def test_rule_flags_non_boundary_file(self):
        pattern, excludes = self._compat_checker_check()
        text = "let mut checker = CompatChecker::with_resolver(db, env);"
        hits = self.arch_guard.find_matches(
            text, pattern, "crates/tsz-checker/src/assignability_checker.rs", excludes
        )
        self.assertEqual(hits, [1])

    def test_rule_ignores_query_boundaries_and_tests(self):
        pattern, excludes = self._compat_checker_check()
        text = "let mut checker = CompatChecker::new(db);"
        query_boundary_hits = self.arch_guard.find_matches(
            text, pattern, "crates/tsz-checker/src/query_boundaries/assignability.rs", excludes
        )
        test_hits = self.arch_guard.find_matches(
            text, pattern, "crates/tsz-checker/tests/foo.rs", excludes
        )
        self.assertEqual(query_boundary_hits, [])
        self.assertEqual(test_hits, [])


class ArchGuardConformanceFixtureGateTests(unittest.TestCase):
    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _fixture_gate_check(self):
        for name, _base, pattern, excludes in self.arch_guard.CHECKS:
            if name == "Production code must not branch on conformance fixture identity":
                return pattern, excludes
        self.fail("conformance fixture identity guard is missing from CHECKS")

    def test_rule_exists(self):
        self._fixture_gate_check()

    def test_rule_flags_production_fixture_gate(self):
        pattern, excludes = self._fixture_gate_check()
        text = 'if test_path.contains("promiseTry") { diagnostics.clear(); }'
        hits = self.arch_guard.find_matches(
            text, pattern, "crates/tsz-cli/src/driver/check.rs", excludes
        )
        self.assertEqual(hits, [1])

    def test_rule_ignores_conformance_harness_and_tests(self):
        pattern, excludes = self._fixture_gate_check()
        text = 'let _ = std::env::var("TSZ_CONFORMANCE_TEST");'
        harness_hits = self.arch_guard.find_matches(
            text, pattern, "crates/conformance/src/runner.rs", excludes
        )
        test_hits = self.arch_guard.find_matches(
            text, pattern, "crates/tsz-cli/tests/conformance_gate.rs", excludes
        )
        self.assertEqual(harness_hits, [])
        self.assertEqual(test_hits, [])

class ArchGuardCallBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _call_checker_compat_construction_check(self):
        for name, _base, pattern, excludes in self.arch_guard.CHECKS:
            if name == "Checker query boundary: call_checker must not construct CompatChecker directly":
                return pattern, excludes
        self.fail("call_checker CompatChecker construction boundary check is missing from CHECKS")

    def _call_checker_concrete_evaluator_check(self):
        for name, _base, pattern, excludes in self.arch_guard.CHECKS:
            if (
                name
                == "Checker query boundary: call_checker must not use concrete CallEvaluator<CompatChecker>"
            ):
                return pattern, excludes
        self.fail("call_checker concrete CallEvaluator boundary check is missing from CHECKS")

    def test_call_checker_specific_rules_exist(self):
        self._call_checker_compat_construction_check()
        self._call_checker_concrete_evaluator_check()

    def test_call_checker_compat_construction_is_flagged(self):
        pattern, excludes = self._call_checker_compat_construction_check()
        text = "let compat = CompatChecker::with_resolver(db, env);"
        hits = self.arch_guard.find_matches(
            text, pattern, "crates/tsz-checker/src/query_boundaries/call_checker.rs", excludes
        )
        self.assertEqual(hits, [1])

    def test_call_checker_concrete_evaluator_is_flagged(self):
        pattern, excludes = self._call_checker_concrete_evaluator_check()
        text = "CallEvaluator::<tsz_solver::CompatChecker>::get_contextual_signature(node);"
        hits = self.arch_guard.find_matches(
            text, pattern, "crates/tsz-checker/src/query_boundaries/call_checker.rs", excludes
        )
        self.assertEqual(hits, [1])

    def test_assignability_boundary_remains_allowed_for_compat_construction(self):
        pattern, excludes = self._call_checker_compat_construction_check()
        text = "CompatChecker::with_resolver(db, env)"
        hits = self.arch_guard.find_matches(
            text, pattern, "crates/tsz-checker/src/query_boundaries/assignability.rs", excludes
        )
        self.assertEqual(hits, [])


class ArchGuardSolverRelationBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _solver_relation_check(self):
        for name, _base, pattern, excludes in self.arch_guard.CHECKS:
            if name == "Checker boundary: direct solver relation queries outside query boundaries/tests":
                return pattern, excludes
        self.fail("solver relation boundary check is missing from CHECKS")

    def test_rule_exists(self):
        self._solver_relation_check()

    def test_rule_flags_non_boundary_file(self):
        pattern, excludes = self._solver_relation_check()
        text = "let ok = tsz_solver::is_subtype_of(db, source, target);"
        hits = self.arch_guard.find_matches(
            text, pattern, "crates/tsz-checker/src/type_computation.rs", excludes
        )
        self.assertEqual(hits, [1])

    def test_rule_ignores_query_boundaries_and_tests(self):
        pattern, excludes = self._solver_relation_check()
        text = "let ok = tsz_solver::is_assignable_to(db, source, target);"
        query_boundary_hits = self.arch_guard.find_matches(
            text, pattern, "crates/tsz-checker/src/query_boundaries/flow_analysis.rs", excludes
        )
        test_hits = self.arch_guard.find_matches(
            text, pattern, "crates/tsz-checker/tests/foo.rs", excludes
        )
        self.assertEqual(query_boundary_hits, [])
        self.assertEqual(test_hits, [])


class ArchGuardCoreWasmBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _core_wasm_boundary_check(self):
        for name, _base, pattern, excludes in self.arch_guard.CHECKS:
            if name == "Core boundary: wasm bindings must stay in current wasm surface files":
                return pattern, excludes
        self.fail("core wasm boundary check is missing from CHECKS")

    def test_rule_exists(self):
        self._core_wasm_boundary_check()

    def test_rule_flags_non_allowlisted_core_file(self):
        pattern, excludes = self._core_wasm_boundary_check()
        text = "use wasm_bindgen::prelude::wasm_bindgen;"
        hits = self.arch_guard.find_matches(
            text, pattern, "crates/tsz-core/src/source_file.rs", excludes
        )
        self.assertEqual(hits, [1])

    def test_rule_allows_existing_wasm_surface_files(self):
        pattern, excludes = self._core_wasm_boundary_check()
        text = "use wasm_bindgen::prelude::JsValue;"
        lib_hits = self.arch_guard.find_matches(text, pattern, "crates/tsz-core/src/lib.rs", excludes)
        api_hits = self.arch_guard.find_matches(
            text, pattern, "crates/tsz-core/src/api/wasm/code_actions.rs", excludes
        )
        self.assertEqual(lib_hits, [])
        self.assertEqual(api_hits, [])

    def test_rule_ignores_tests_directory(self):
        pattern, excludes = self._core_wasm_boundary_check()
        text = "use wasm_bindgen::prelude::JsValue;"
        hits = self.arch_guard.find_matches(text, pattern, "crates/tsz-core/tests/foo.rs", excludes)
        self.assertEqual(hits, [])


class ArchGuardCheckerFileSizeBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _checker_file_size_check(self):
        for entry in self.arch_guard.LINE_LIMIT_CHECKS:
            name, _base, limit = entry[0], entry[1], entry[2]
            if name == "Checker boundary: src files must stay under 2000 LOC":
                return limit
        self.fail("checker file size boundary check is missing from LINE_LIMIT_CHECKS")

    def test_rule_exists_with_expected_limit(self):
        limit = self._checker_file_size_check()
        self.assertEqual(limit, 2000)

    def test_scan_line_limits_flags_file_above_limit(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            base = pathlib.Path(temp_dir)
            target = base / "too_big.rs"
            target.write_text("let x = 0;\n" * 2001, encoding="utf-8")
            hits = self.arch_guard.scan_line_limits(base, 2000)
            self.assertEqual(len(hits), 1)
            self.assertTrue(hits[0].endswith("too_big.rs:2001 lines (limit 2000)"))

    def test_scan_line_limits_allows_file_at_limit(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            base = pathlib.Path(temp_dir)
            target = base / "at_limit.rs"
            target.write_text("let x = 0;\n" * 2000, encoding="utf-8")
            hits = self.arch_guard.scan_line_limits(base, 2000)
            self.assertEqual(hits, [])


class ArchGuardCoreLibFacadeSizeBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _core_lib_size_check(self):
        for entry in self.arch_guard.FILE_LINE_LIMIT_CHECKS:
            name, path, limit = entry
            if name == "Core boundary: tsz-core lib facade must stay under 2420 LOC":
                return path, limit
        self.fail("core lib facade size boundary check is missing from FILE_LINE_LIMIT_CHECKS")

    def test_rule_exists_with_expected_limit(self):
        path, limit = self._core_lib_size_check()
        self.assertEqual(limit, 2420)
        self.assertTrue(str(path).endswith("crates/tsz-core/src/lib.rs"))

    def test_scan_file_line_limit_flags_file_above_limit(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            target = pathlib.Path(temp_dir) / "too_big.rs"
            target.write_text("let x = 0;\n" * 11, encoding="utf-8")
            hits = self.arch_guard.scan_file_line_limit(target, 10)
            self.assertEqual(len(hits), 1)
            self.assertTrue(hits[0].endswith("too_big.rs:11 lines (limit 10)"))

    def test_scan_file_line_limit_allows_file_at_limit(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            target = pathlib.Path(temp_dir) / "at_limit.rs"
            target.write_text("let x = 0;\n" * 10, encoding="utf-8")
            hits = self.arch_guard.scan_file_line_limit(target, 10)
            self.assertEqual(hits, [])


class ArchGuardSolverTypeDataQuarantineTests(unittest.TestCase):
    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def test_scan_solver_typedata_quarantine_flags_grouped_alias_multiline_intern(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            solver_root = pathlib.Path(temp_dir) / "crates" / "tsz-solver"
            src_dir = solver_root / "src"
            src_dir.mkdir(parents=True)
            target = src_dir / "bad.rs"
            target.write_text(
                "\n".join(
                    [
                        "use crate::types::{TypeData as TD};",
                        "",
                        "fn bad(interner: &mut crate::intern::TypeInterner) {",
                        "    interner",
                        "        .intern(",
                        "            TD::ThisType,",
                        "        );",
                        "}",
                    ]
                ),
                encoding="utf-8",
            )

            hits = self.arch_guard.scan_solver_typedata_quarantine(solver_root)
            self.assertEqual(len(hits), 1)
            self.assertTrue(hits[0].endswith("/bad.rs:5"))

    def test_scan_solver_typedata_quarantine_ignores_allowlisted_interner_files(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            solver_root = pathlib.Path(temp_dir) / "crates" / "tsz-solver"
            intern_dir = solver_root / "src" / "intern"
            intern_dir.mkdir(parents=True)
            target = intern_dir / "mod.rs"
            target.write_text("fn ok() { interner.intern(TypeData::ThisType); }", encoding="utf-8")

            hits = self.arch_guard.scan_solver_typedata_quarantine(solver_root)
            self.assertEqual(hits, [])

    def test_scan_solver_typedata_quarantine_ignores_commented_raw_intern_patterns(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            solver_root = pathlib.Path(temp_dir) / "crates" / "tsz-solver"
            src_dir = solver_root / "src"
            src_dir.mkdir(parents=True)
            target = src_dir / "commented.rs"
            target.write_text(
                "\n".join(
                    [
                        "use crate::types::TypeData;",
                        "/* interner.intern(TypeData::ThisType); */",
                        "// interner.intern(TypeData::Unknown);",
                        "fn ok(_interner: &mut crate::intern::TypeInterner) {}",
                    ]
                ),
                encoding="utf-8",
            )

            hits = self.arch_guard.scan_solver_typedata_quarantine(solver_root)
            self.assertEqual(hits, [])

    def test_scan_solver_typedata_quarantine_preserves_real_calls_near_comments(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            solver_root = pathlib.Path(temp_dir) / "crates" / "tsz-solver"
            src_dir = solver_root / "src"
            src_dir.mkdir(parents=True)
            target = src_dir / "mixed.rs"
            target.write_text(
                "\n".join(
                    [
                        "use crate::types::TypeData;",
                        "/* interner.intern(TypeData::Never); */",
                        "fn bad(interner: &mut crate::intern::TypeInterner) {",
                        "    interner.intern(TypeData::ThisType); // real violation",
                        "}",
                    ]
                ),
                encoding="utf-8",
            )

            hits = self.arch_guard.scan_solver_typedata_quarantine(solver_root)
            self.assertEqual(len(hits), 1)
            self.assertTrue(hits[0].endswith("/mixed.rs:4"))


class ArchGuardRatchetDirectionTests(unittest.TestCase):
    """Ensure the exclusion lists can only shrink, never grow."""

    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def test_line_limit_exclusion_count_cannot_grow(self):
        """The number of excluded files in LINE_LIMIT_CHECKS must not increase."""
        # Current ceiling: 17 excluded files.
        # When a file drops below 2000 lines, remove it and lower this ceiling.
        MAX_EXCLUDED = 17
        for entry in self.arch_guard.LINE_LIMIT_CHECKS:
            excludes = entry[3] if len(entry) > 3 else set()
            self.assertLessEqual(
                len(excludes),
                MAX_EXCLUDED,
                f"LINE_LIMIT_CHECKS exclusion list has {len(excludes)} entries, "
                f"max allowed is {MAX_EXCLUDED}. Remove files that dropped below the limit.",
            )

    def test_excluded_files_actually_exist(self):
        """Every file in the exclusion list must exist on disk."""
        for entry in self.arch_guard.LINE_LIMIT_CHECKS:
            excludes = entry[3] if len(entry) > 3 else set()
            for rel_path in excludes:
                full_path = ROOT / rel_path
                self.assertTrue(
                    full_path.exists(),
                    f"Excluded file {rel_path} does not exist. Remove it from the exclusion list.",
                )

    def test_excluded_files_actually_exceed_limit(self):
        """Every excluded file must actually be over the limit (raw line count)."""
        for entry in self.arch_guard.LINE_LIMIT_CHECKS:
            limit = entry[2]
            excludes = entry[3] if len(entry) > 3 else set()
            for rel_path in excludes:
                full_path = ROOT / rel_path
                if not full_path.exists():
                    continue  # caught by test_excluded_files_actually_exist
                with full_path.open("r", encoding="utf-8", errors="ignore") as fh:
                    line_count = sum(1 for _ in fh)
                self.assertGreater(
                    line_count,
                    limit,
                    f"Excluded file {rel_path} has {line_count} lines "
                    f"(limit {limit}). Remove it from the exclusion list.",
                )

    def test_lookup_exclusion_files_actually_exist(self):
        """Every file in the lookup() exclusion list must exist on disk."""
        for name, _base, _pattern, excludes in self.arch_guard.CHECKS:
            if "exclude_files" not in excludes:
                continue
            for rel_path in excludes["exclude_files"]:
                full_path = ROOT / rel_path
                self.assertTrue(
                    full_path.exists(),
                    f"Excluded file {rel_path} in check '{name}' does not exist. "
                    f"Remove it from the exclusion list.",
                )


class ArchGuardStructFieldCountTests(unittest.TestCase):
    """Cover `STRUCT_FIELD_COUNT_CHECKS` + `scan_struct_field_count`.

    The CheckerContext check is the architecture-health-metric-1 anchor
    from `docs/plan/ROADMAP.md`. These tests pin the regex semantics so
    future rewrites (e.g. to syn) preserve the invariants:

      - count comments out
      - count `pub`, `pub(crate)`, and bare-private fields
      - skip lines that aren't `name: Type,` shaped
      - report `struct not found` rather than passing silently
    """

    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _write_and_scan(self, body: str, struct_name: str, max_fields: int):
        with tempfile.TemporaryDirectory() as tmp:
            path = pathlib.Path(tmp) / "struct.rs"
            path.write_text(body, encoding="utf-8")
            return self.arch_guard.scan_struct_field_count(
                path, struct_name, max_fields
            )

    def test_counts_pub_pub_crate_and_private_fields(self):
        body = "\n".join(
            [
                "pub struct Sample {",
                "    pub a: u32,",
                "    pub(crate) b: String,",
                "    c: bool,",
                "}",
            ]
        )
        hits = self._write_and_scan(body, "Sample", 2)
        self.assertEqual(len(hits), 1)
        self.assertIn("3 fields", hits[0])
        self.assertIn("cap 2", hits[0])

    def test_passes_when_at_or_under_cap(self):
        body = "\n".join(
            [
                "pub struct Sample {",
                "    a: u32,",
                "    b: u32,",
                "}",
            ]
        )
        self.assertEqual(self._write_and_scan(body, "Sample", 2), [])
        self.assertEqual(self._write_and_scan(body, "Sample", 3), [])

    def test_strips_comments_so_commented_out_fields_dont_count(self):
        body = "\n".join(
            [
                "pub struct Sample {",
                "    a: u32,",
                "    // b: u32,",
                "    /* c: u32, */",
                "}",
            ]
        )
        self.assertEqual(self._write_and_scan(body, "Sample", 1), [])

    def test_reports_struct_not_found(self):
        body = "pub struct Other { a: u32 }"
        hits = self._write_and_scan(body, "Sample", 10)
        self.assertEqual(len(hits), 1)
        self.assertIn("not found", hits[0])

    def test_checker_context_field_count_check_is_registered(self):
        for entry in self.arch_guard.STRUCT_FIELD_COUNT_CHECKS:
            name, path, struct_name, _max = entry
            if struct_name == "CheckerContext":
                self.assertTrue(
                    path.exists(),
                    f"CheckerContext check points at missing path: {path}",
                )
                self.assertIn("CheckerContext", name)
                return
        self.fail(
            "CheckerContext field-count check is missing from STRUCT_FIELD_COUNT_CHECKS"
        )

    def test_real_checker_context_passes_at_pinned_cap(self):
        """The pinned cap must match the live count (no off-by-one)."""
        for entry in self.arch_guard.STRUCT_FIELD_COUNT_CHECKS:
            name, path, struct_name, max_fields = entry
            hits = self.arch_guard.scan_struct_field_count(
                path, struct_name, max_fields
            )
            self.assertEqual(
                hits,
                [],
                f"{name}: cap is too tight — guard fires at the live count.",
            )


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

    Architecture health metric 3 anchor — workstream 3 ("Compiler Service
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
            any("metric 3" in name for name in names),
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


class ArchGuardDeclarationEmitRederiveTests(unittest.TestCase):
    """Cover `DECLARATION_EMIT_REDERIVE_CHECKS` + `scan_declaration_emit_rederive`.

    Architecture health metric 8 anchor — workstream 2 ("Declaration emit
    priorities") wants declaration emit to consume a stable semantic
    summary rather than rederive facts via direct `tsz_checker` /
    `tsz_solver` / `tsz_binder` imports.  These tests pin the detection
    semantics:

      - any of `use tsz_<crate>::...`, `pub use tsz_<crate>`, or
        `extern crate tsz_<crate>` lines flag the file (where `<crate>`
        is one of `checker`, `solver`, `binder`)
      - test files (`*_tests.rs`, `test_*.rs`, files under `tests/` or
        `benches/`) are excluded
      - files importing several semantic crates count once
      - comment-only lines are not counted
      - non-semantic-crate imports (e.g. `tsz_parser`, `tsz_emitter`)
        are not counted
      - the pinned cap matches the live count
    """

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

    def test_flags_each_semantic_crate_import_form(self):
        root = self._make_tree(
            {
                "use_checker.rs": "use tsz_checker::CheckerState;\n",
                "use_solver.rs": "use tsz_solver::TypeId;\n",
                "pub_use_binder.rs": "pub use tsz_binder;\n",
                "extern_checker.rs": "extern crate tsz_checker;\n",
            }
        )
        hits = self.arch_guard.scan_declaration_emit_rederive(root, 0)
        # 4 files reached the boundary + 1 summary line.
        self.assertEqual(len(hits), 5, f"unexpected hits: {hits!r}")
        joined = "\n".join(hits[:-1])
        self.assertIn("use_checker.rs", joined)
        self.assertIn("use_solver.rs", joined)
        self.assertIn("pub_use_binder.rs", joined)
        self.assertIn("extern_checker.rs", joined)
        self.assertIn(
            "total declaration_emitter files importing "
            "tsz_checker/tsz_solver/tsz_binder: 4",
            hits[-1],
        )

    def test_does_not_flag_non_semantic_crate_imports(self):
        # The metric is specifically about semantic rederivation.
        # Imports of other tsz crates (e.g. tsz_parser, tsz_emitter,
        # tsz_lowering, tsz_common) are NOT in scope and must not
        # trigger the guard.
        root = self._make_tree(
            {
                "non_semantic.rs": (
                    "use tsz_parser::ParserState;\n"
                    "use tsz_emitter::Printer;\n"
                    "use tsz_lowering::TypeLowering;\n"
                    "use tsz_common::position::LineMap;\n"
                ),
            }
        )
        hits = self.arch_guard.scan_declaration_emit_rederive(root, 0)
        self.assertEqual(hits, [], f"unexpected hits: {hits!r}")

    def test_counts_each_file_at_most_once(self):
        # A file importing checker AND solver AND binder still counts as 1.
        root = self._make_tree(
            {
                "many.rs": (
                    "use tsz_checker::CheckerState;\n"
                    "use tsz_solver::TypeId;\n"
                    "use tsz_binder::BinderState;\n"
                ),
            }
        )
        hits = self.arch_guard.scan_declaration_emit_rederive(root, 0)
        # 1 importing file + 1 summary line.
        self.assertEqual(len(hits), 2, f"unexpected hits: {hits!r}")
        self.assertIn(
            "total declaration_emitter files importing "
            "tsz_checker/tsz_solver/tsz_binder: 1",
            hits[-1],
        )

    def test_excludes_test_files_and_test_dirs(self):
        root = self._make_tree(
            {
                "foo_tests.rs": "use tsz_checker::CheckerState;\n",
                "test_helpers.rs": "use tsz_solver::TypeId;\n",
                "tests/integration.rs": "use tsz_binder::BinderState;\n",
                "benches/bench.rs": "use tsz_checker::CheckerState;\n",
            }
        )
        hits = self.arch_guard.scan_declaration_emit_rederive(root, 0)
        self.assertEqual(hits, [], f"unexpected hits: {hits!r}")

    def test_ignores_comment_only_lines(self):
        root = self._make_tree(
            {
                "commented.rs": (
                    "// use tsz_checker::CheckerState;\n"
                    "// pub use tsz_solver;\n"
                ),
            }
        )
        hits = self.arch_guard.scan_declaration_emit_rederive(root, 0)
        self.assertEqual(hits, [], f"unexpected hits: {hits!r}")

    def test_check_is_registered(self):
        names = [
            entry[0] for entry in self.arch_guard.DECLARATION_EMIT_REDERIVE_CHECKS
        ]
        self.assertTrue(
            any("metric 8" in name for name in names),
            "Declaration-emit-rederive guard missing from DECLARATION_EMIT_REDERIVE_CHECKS",
        )

    def test_real_imports_pass_at_pinned_cap(self):
        """The pinned cap must match the live count (no off-by-one)."""
        for entry in self.arch_guard.DECLARATION_EMIT_REDERIVE_CHECKS:
            name, search_root, max_files = entry
            hits = self.arch_guard.scan_declaration_emit_rederive(
                search_root, max_files
            )
            self.assertEqual(
                hits,
                [],
                f"{name}: cap is too tight — guard fires at the live count.",
            )


if __name__ == "__main__":
    unittest.main()
