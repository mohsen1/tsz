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

    def _solver_relation_policy_check(self):
        for name, _base, pattern, excludes in self.arch_guard.CHECKS:
            if (
                name
                == "Checker boundary: direct RelationPolicy/RelationContext usage outside query boundaries/tests"
            ):
                return pattern, excludes
        self.fail("solver relation policy boundary check is missing from CHECKS")

    def test_rule_exists(self):
        self._solver_relation_check()
        self._solver_relation_policy_check()

    def test_rule_flags_non_boundary_file(self):
        pattern, excludes = self._solver_relation_check()
        text = "let ok = tsz_solver::is_subtype_of(db, source, target);"
        hits = self.arch_guard.find_matches(
            text, pattern, "crates/tsz-checker/src/type_computation.rs", excludes
        )
        self.assertEqual(hits, [1])

    def test_rule_flags_relation_policy_and_context_usage(self):
        pattern, excludes = self._solver_relation_policy_check()
        text = (
            "let policy = tsz_solver::RelationPolicy::diagnostic_default();\n"
            "let ctx = tsz_solver::RelationContext::default();\n"
            "use tsz_solver::{RelationPolicy, TypeId};\n"
        )
        hits = self.arch_guard.find_matches(
            text,
            pattern,
            "crates/tsz-checker/src/error_reporter/diagnostic.rs",
            excludes,
        )
        self.assertEqual(hits, [1, 2, 3])

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

        pattern, excludes = self._solver_relation_policy_check()
        text = "let policy = tsz_solver::RelationPolicy::diagnostic_default();"
        query_boundary_hits = self.arch_guard.find_matches(
            text, pattern, "crates/tsz-checker/src/query_boundaries/assignability.rs", excludes
        )
        test_hits = self.arch_guard.find_matches(
            text, pattern, "crates/tsz-checker/tests/relation_policy.rs", excludes
        )
        self.assertEqual(query_boundary_hits, [])
        self.assertEqual(test_hits, [])


class ArchGuardBinaryEvaluatorBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _binary_evaluator_surface_check(self):
        for name, _base, pattern, excludes in self.arch_guard.CHECKS:
            if name == "Checker type-computation boundary: no direct BinaryOpEvaluator surface (#8226)":
                return pattern, excludes
        self.fail("BinaryOpEvaluator boundary check is missing from CHECKS")

    def test_rule_exists(self):
        self._binary_evaluator_surface_check()

    def test_rule_flags_imports_and_signatures_outside_boundary(self):
        pattern, excludes = self._binary_evaluator_surface_check()
        text = "\n".join(
            [
                "use tsz_solver::computation::BinaryOpEvaluator;",
                "fn helper(evaluator: &BinaryOpEvaluator) {}",
            ]
        )
        hits = self.arch_guard.find_matches(
            text, pattern, "crates/tsz-checker/src/types/computation/binary.rs", excludes
        )
        self.assertEqual(hits, [1, 2])

    def test_rule_ignores_query_boundaries_tests_and_comments(self):
        pattern, excludes = self._binary_evaluator_surface_check()
        text = "/// `BinaryOpEvaluator` is documented here\nlet evaluator = BinaryOpEvaluator;"
        query_boundary_hits = self.arch_guard.find_matches(
            text, pattern, "crates/tsz-checker/src/query_boundaries/common.rs", excludes
        )
        test_hits = self.arch_guard.find_matches(
            text, pattern, "crates/tsz-checker/src/tests/architecture_contract_tests.rs", excludes
        )
        comment_hits = self.arch_guard.find_matches(
            "/// `BinaryOpEvaluator` comment only",
            pattern,
            "crates/tsz-checker/src/types/computation/binary.rs",
            excludes,
        )
        self.assertEqual(query_boundary_hits, [])
        self.assertEqual(test_hits, [])
        self.assertEqual(comment_hits, [])


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


class ArchGuardCheckerComputationFileSizeBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _computation_file_size_check(self):
        for entry in self.arch_guard.LINE_LIMIT_CHECKS:
            name, base, limit = entry[0], entry[1], entry[2]
            if name == (
                "Checker computation boundary: type-computation monoliths "
                "must stay below 3200 LOC (#8226)"
            ):
                return base, limit
        self.fail(
            "checker type-computation size boundary check is missing from "
            "LINE_LIMIT_CHECKS"
        )

    def test_rule_exists_with_expected_limit(self):
        base, limit = self._computation_file_size_check()
        self.assertEqual(limit, 3200)
        self.assertTrue(
            str(base).endswith("crates/tsz-checker/src/types/computation")
        )

    def test_real_type_computation_files_pass_at_pinned_limit(self):
        base, limit = self._computation_file_size_check()
        hits = self.arch_guard.scan_line_limits(base, limit)
        self.assertEqual(
            hits,
            [],
            "type-computation monolith cap is too tight for the live files",
        )


class ArchGuardCoreLibFacadeSizeBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _core_lib_size_check(self):
        for entry in self.arch_guard.FILE_LINE_LIMIT_CHECKS:
            name, path, limit = entry
            if name == "Core boundary: tsz-core lib facade must stay under 500 LOC":
                return path, limit
        self.fail("core lib facade size boundary check is missing from FILE_LINE_LIMIT_CHECKS")

    def test_rule_exists_with_expected_limit(self):
        path, limit = self._core_lib_size_check()
        self.assertEqual(limit, 500)
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


class ArchGuardQueryBoundaryCommonSizeTests(unittest.TestCase):
    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _query_common_size_check(self):
        for entry in self.arch_guard.FILE_LINE_LIMIT_CHECKS:
            name, path, limit = entry
            if name == "Checker query boundary: common quarantine must not grow (#8225)":
                return path, limit
        self.fail("query boundary common size check is missing from FILE_LINE_LIMIT_CHECKS")

    def test_rule_exists_with_current_limit(self):
        path, limit = self._query_common_size_check()
        self.assertEqual(limit, 1996)
        self.assertTrue(
            str(path).endswith("crates/tsz-checker/src/query_boundaries/common.rs")
        )

    def test_real_common_file_passes_at_pinned_limit(self):
        path, limit = self._query_common_size_check()
        hits = self.arch_guard.scan_file_line_limit(path, limit)
        self.assertEqual(
            hits,
            [],
            "query_boundaries/common.rs cap is too tight for the live file",
        )


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
        # Current ceiling: 38 excluded files.
        # When a file drops below 2000 lines, remove it and lower this ceiling.
        # 2026-05-01: ratcheted from 17 → 35 after the inherited list went
        # to 66 entries (mostly stale). The 35 set now matches the actual
        # at-or-above-2000-LOC files on disk; new entries should be rare,
        # and removals (file splits) should ratchet this number down.
        # 2026-05-05: 35 → 36 after `crates/tsz-checker/src/checkers/jsx/props/
        # resolution.rs` crossed the 2000-LOC threshold (2001 lines after #2717).
        # 2026-05-12: pruned stale under-limit files and re-pinned the live
        # audited over-limit set at 38.
        MAX_EXCLUDED = 38
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


class ArchGuardTraitMethodCountTests(unittest.TestCase):
    """Cover `TRAIT_METHOD_COUNT_CHECKS` + `scan_trait_method_count`.

    The `TypeDatabase` check is the #8205 solver boundary ratchet: the current
    broad trait is tolerated as baseline debt, but its capability surface must
    not grow while narrower storage/config/provenance traits are extracted.
    """

    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _write_and_scan(self, body: str, trait_name: str, max_methods: int):
        with tempfile.TemporaryDirectory() as tmp:
            path = pathlib.Path(tmp) / "trait.rs"
            path.write_text(body, encoding="utf-8")
            return self.arch_guard.scan_trait_method_count(
                path, trait_name, max_methods
            )

    def test_counts_required_and_default_methods(self):
        body = "\n".join(
            [
                "pub trait Sample {",
                "    fn lookup(&self);",
                "    fn construct(&self) {}",
                "    unsafe fn raw(&self);",
                "}",
            ]
        )
        hits = self._write_and_scan(body, "Sample", 2)
        self.assertEqual(len(hits), 1)
        self.assertIn("3 methods", hits[0])
        self.assertIn("cap 2", hits[0])

    def test_passes_when_at_or_under_cap(self):
        body = "\n".join(
            [
                "pub trait Sample {",
                "    fn a(&self);",
                "    fn b(&self) {}",
                "}",
            ]
        )
        self.assertEqual(self._write_and_scan(body, "Sample", 2), [])
        self.assertEqual(self._write_and_scan(body, "Sample", 3), [])

    def test_strips_comments_and_handles_nested_default_body(self):
        body = "\n".join(
            [
                "pub trait Sample {",
                "    fn a(&self) {",
                "        if true {",
                "            let _x = 1;",
                "        }",
                "    }",
                "    // fn b(&self);",
                "    /* fn c(&self); */",
                "}",
            ]
        )
        self.assertEqual(self._write_and_scan(body, "Sample", 1), [])

    def test_reports_trait_not_found(self):
        body = "pub trait Other { fn a(&self); }"
        hits = self._write_and_scan(body, "Sample", 10)
        self.assertEqual(len(hits), 1)
        self.assertIn("not found", hits[0])

    def test_typedatabase_method_count_check_is_registered(self):
        for entry in self.arch_guard.TRAIT_METHOD_COUNT_CHECKS:
            name, path, trait_name, _max = entry
            if trait_name == "TypeDatabase":
                self.assertTrue(
                    path.exists(),
                    f"TypeDatabase check points at missing path: {path}",
                )
                self.assertIn("#8205", name)
                return
        self.fail(
            "TypeDatabase method-count check is missing from TRAIT_METHOD_COUNT_CHECKS"
        )

    def test_real_typedatabase_passes_at_pinned_cap(self):
        """The pinned cap must match the live count (no off-by-one)."""
        for entry in self.arch_guard.TRAIT_METHOD_COUNT_CHECKS:
            name, path, trait_name, max_methods = entry
            hits = self.arch_guard.scan_trait_method_count(
                path, trait_name, max_methods
            )
            self.assertEqual(
                hits,
                [],
                f"{name}: cap is too tight — guard fires at the live count.",
            )


class ArchGuardCheckerContextLifetimeManifestTests(unittest.TestCase):
    """Cover the T2.1.A CheckerContext lifetime inventory guard."""

    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _write_and_scan(self, struct_body: str, manifest_body: str):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            struct_path = root / "context.rs"
            manifest_path = root / "checker_context_lifetimes.toml"
            struct_path.write_text(struct_body, encoding="utf-8")
            manifest_path.write_text(manifest_body, encoding="utf-8")
            return self.arch_guard.scan_checker_context_lifetime_manifest(
                struct_path, "CheckerContext", manifest_path
            )

    def test_valid_manifest_passes(self):
        struct_body = "\n".join(
            [
                "pub struct CheckerContext<'a> {",
                "    pub arena: &'a NodeArena,",
                "    request_node_types: FxHashMap<u32, TypeId>,",
                "}",
            ]
        )
        manifest_body = "\n".join(
            [
                "[arena]",
                'lifetime = "FileLocalReset"',
                'capability = "CheckerInputs"',
                'reason = "borrowed current-file arena"',
                "",
                "[request_node_types]",
                'lifetime = "SpeculationScoped"',
                'capability = "SpeculationState"',
                'reason = "snapshot by speculative return-type inference"',
            ]
        )
        self.assertEqual(self._write_and_scan(struct_body, manifest_body), [])

    def test_inline_manifest_entries_pass(self):
        struct_body = "\n".join(
            [
                "pub struct CheckerContext {",
                "    pub arena: NodeArena,",
                "    pub binder: BinderState,",
                "}",
            ]
        )
        manifest_body = "\n".join(
            [
                'arena = { lifetime = "FileLocalReset", capability = "CheckerInputs", reason = "current arena" }',
                'binder = { lifetime = "FileLocalReset", capability = "CheckerInputs", reason = "current binder" }',
            ]
        )
        self.assertEqual(self._write_and_scan(struct_body, manifest_body), [])

    def test_missing_struct_field_is_reported(self):
        struct_body = "\n".join(
            [
                "pub struct CheckerContext {",
                "    pub arena: NodeArena,",
                "    pub binder: BinderState,",
                "}",
            ]
        )
        manifest_body = "\n".join(
            [
                "[arena]",
                'lifetime = "FileLocalReset"',
                'capability = "CheckerInputs"',
                'reason = "borrowed current-file arena"',
            ]
        )
        hits = self._write_and_scan(struct_body, manifest_body)
        self.assertEqual(len(hits), 1)
        self.assertIn("missing CheckerContext lifetime for field [binder]", hits[0])

    def test_stale_manifest_entry_is_reported(self):
        struct_body = "\n".join(
            [
                "pub struct CheckerContext {",
                "    pub arena: NodeArena,",
                "}",
            ]
        )
        manifest_body = "\n".join(
            [
                "[arena]",
                'lifetime = "FileLocalReset"',
                'capability = "CheckerInputs"',
                'reason = "borrowed current-file arena"',
                "",
                "[removed_field]",
                'lifetime = "FileLocalReset"',
                'capability = "FileTypeCache"',
                'reason = "old field"',
            ]
        )
        hits = self._write_and_scan(struct_body, manifest_body)
        self.assertEqual(len(hits), 1)
        self.assertIn("stale manifest entry [removed_field]", hits[0])

    def test_unknown_lifetime_is_reported(self):
        struct_body = "pub struct CheckerContext { pub arena: NodeArena, }"
        manifest_body = "\n".join(
            [
                "[arena]",
                'lifetime = "Unknown"',
                'capability = "CheckerInputs"',
                'reason = "unclassified"',
            ]
        )
        hits = self._write_and_scan(struct_body, manifest_body)
        self.assertEqual(len(hits), 1)
        self.assertIn("lifetime must not be Unknown", hits[0])

    def test_invalid_lifetime_is_reported(self):
        struct_body = "pub struct CheckerContext { pub arena: NodeArena, }"
        manifest_body = "\n".join(
            [
                "[arena]",
                'lifetime = "ForeverCache"',
                'capability = "CheckerInputs"',
                'reason = "invalid class"',
            ]
        )
        hits = self._write_and_scan(struct_body, manifest_body)
        self.assertEqual(len(hits), 1)
        self.assertIn("invalid lifetime 'ForeverCache'", hits[0])

    def test_missing_capability_is_reported(self):
        struct_body = "pub struct CheckerContext { pub arena: NodeArena, }"
        manifest_body = "\n".join(
            [
                "[arena]",
                'lifetime = "FileLocalReset"',
                'reason = "borrowed current-file arena"',
            ]
        )
        hits = self._write_and_scan(struct_body, manifest_body)
        self.assertEqual(len(hits), 1)
        self.assertIn("[arena] missing capability", hits[0])

    def test_unknown_capability_is_reported(self):
        struct_body = "pub struct CheckerContext { pub arena: NodeArena, }"
        manifest_body = "\n".join(
            [
                "[arena]",
                'lifetime = "FileLocalReset"',
                'capability = "Unknown"',
                'reason = "borrowed current-file arena"',
            ]
        )
        hits = self._write_and_scan(struct_body, manifest_body)
        self.assertEqual(len(hits), 1)
        self.assertIn("[arena] capability must not be Unknown", hits[0])

    def test_invalid_capability_is_reported(self):
        struct_body = "pub struct CheckerContext { pub arena: NodeArena, }"
        manifest_body = "\n".join(
            [
                "[arena]",
                'lifetime = "FileLocalReset"',
                'capability = "GlobalBag"',
                'reason = "borrowed current-file arena"',
            ]
        )
        hits = self._write_and_scan(struct_body, manifest_body)
        self.assertEqual(len(hits), 1)
        self.assertIn("invalid capability 'GlobalBag'", hits[0])

    def test_missing_reason_is_reported(self):
        struct_body = "pub struct CheckerContext { pub arena: NodeArena, }"
        manifest_body = "\n".join(
            [
                "[arena]",
                'lifetime = "FileLocalReset"',
                'capability = "CheckerInputs"',
            ]
        )
        hits = self._write_and_scan(struct_body, manifest_body)
        self.assertEqual(len(hits), 1)
        self.assertIn("[arena] missing reason", hits[0])

    def test_checker_context_lifetime_check_is_registered(self):
        for entry in self.arch_guard.CHECKER_CONTEXT_LIFETIME_MANIFEST_CHECKS:
            name, struct_path, struct_name, manifest_path = entry
            if struct_name == "CheckerContext":
                self.assertTrue(
                    struct_path.exists(),
                    f"CheckerContext lifetime check points at missing path: {struct_path}",
                )
                self.assertIn("CheckerContext", name)
                self.assertTrue(
                    manifest_path.parent.exists(),
                    f"CheckerContext lifetime manifest parent is missing: {manifest_path}",
                )
                return
        self.fail(
            "CheckerContext lifetime check is missing from "
            "CHECKER_CONTEXT_LIFETIME_MANIFEST_CHECKS"
        )

    def test_real_checker_context_lifetime_manifest_passes(self):
        for entry in self.arch_guard.CHECKER_CONTEXT_LIFETIME_MANIFEST_CHECKS:
            name, struct_path, struct_name, manifest_path = entry
            hits = self.arch_guard.scan_checker_context_lifetime_manifest(
                struct_path, struct_name, manifest_path
            )
            self.assertEqual(hits, [], f"{name}: {hits[:5]}")


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
        """The pinned cap must match the live count (no off-by-one)."""
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


class ArchGuardProjectDashboardRowTests(unittest.TestCase):
    """Cover Track 1 project dashboard row inventory checks.

    The public compatibility dashboard must render every row the benchmark
    artifact expects or compile-canary CI tracks. This keeps
    `COMPATIBILITY_CORPUS_ROWS` from drifting behind
    `REQUIRED_PROJECT_ROWS` / `COMPILE_CANARY_PROJECT_ROWS`.
    """

    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _write_and_scan(self, body: str):
        with tempfile.TemporaryDirectory() as tmp:
            path = pathlib.Path(tmp) / "project-rows.mjs"
            path.write_text(body, encoding="utf-8")
            return self.arch_guard.scan_project_dashboard_rows(path)

    def test_matching_expected_and_canary_rows_pass(self):
        body = """
const REQUIRED_PROJECT_ROWS = ["utility-types-project"];
export const COMPILE_CANARY_PROJECT_ROWS = ["zod-project"];
export const COMPATIBILITY_CORPUS_ROWS = [
  { name: "utility-types-project", label: "utility-types" },
  { name: "zod-project", label: "Zod" },
];
"""
        self.assertEqual(self._write_and_scan(body), [])

    def test_shared_project_row_definitions_pass(self):
        body = """
export const PROJECT_ROW_DEFINITIONS = [
  {
    name: "utility-types-project",
    benchmark_set: "required",
    guard_set: "required",
  },
  {
    name: "zod-project",
    benchmark_set: "canary",
    guard_set: "canary",
  },
];

export const REQUIRED_PROJECT_ROWS = PROJECT_ROW_DEFINITIONS
  .filter((row) => row.benchmark_set === "required")
  .map((row) => row.name);

export const COMPILE_CANARY_PROJECT_ROWS = PROJECT_ROW_DEFINITIONS
  .filter((row) => row.guard_set === "canary")
  .map((row) => row.name);

export const COMPATIBILITY_CORPUS_ROWS = PROJECT_ROW_DEFINITIONS.map((row) => ({
  name: row.name,
  label: row.label,
}));
"""
        self.assertEqual(self._write_and_scan(body), [])

    def test_missing_dashboard_row_is_reported(self):
        body = """
export const REQUIRED_PROJECT_ROWS = ["utility-types-project"];
export const COMPILE_CANARY_PROJECT_ROWS = ["zod-project"];
export const COMPATIBILITY_CORPUS_ROWS = [
  { name: "zod-project", label: "Zod" },
];
"""
        hits = self._write_and_scan(body)
        self.assertEqual(len(hits), 1)
        self.assertIn("missing compatibility dashboard row for utility-types-project", hits[0])

    def test_stale_dashboard_row_is_reported(self):
        body = """
export const REQUIRED_PROJECT_ROWS = ["utility-types-project"];
export const COMPILE_CANARY_PROJECT_ROWS = [];
export const COMPATIBILITY_CORPUS_ROWS = [
  { name: "utility-types-project", label: "utility-types" },
  { name: "removed-project", label: "removed" },
];
"""
        hits = self._write_and_scan(body)
        self.assertEqual(len(hits), 1)
        self.assertIn("stale compatibility dashboard row for removed-project", hits[0])

    def test_duplicate_dashboard_row_is_reported(self):
        body = """
export const REQUIRED_PROJECT_ROWS = ["utility-types-project"];
export const COMPILE_CANARY_PROJECT_ROWS = [];
export const COMPATIBILITY_CORPUS_ROWS = [
  { name: "utility-types-project", label: "utility-types" },
  { name: "utility-types-project", label: "utility-types again" },
];
"""
        hits = self._write_and_scan(body)
        self.assertEqual(len(hits), 1)
        self.assertIn("duplicate compatibility dashboard row for utility-types-project", hits[0])

    def test_check_is_registered(self):
        names = [entry[0] for entry in self.arch_guard.PROJECT_DASHBOARD_ROW_CHECKS]
        self.assertTrue(
            any("Track 1" in name for name in names),
            "Project dashboard row check missing from PROJECT_DASHBOARD_ROW_CHECKS",
        )

    def test_real_project_dashboard_rows_cover_expected_rows(self):
        for name, file_path in self.arch_guard.PROJECT_DASHBOARD_ROW_CHECKS:
            hits = self.arch_guard.scan_project_dashboard_rows(file_path)
            self.assertEqual(hits, [], f"{name}: {hits[:5]}")


class ArchGuardProjectFixtureSourceTests(unittest.TestCase):
    """Cover Track 1 fixture source/ref metadata checks."""

    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _write_and_scan(self, rows_body: str, fixtures_body: str):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            row_path = root / "project-rows.mjs"
            fixture_path = root / "project-fixtures.sh"
            row_path.write_text(rows_body, encoding="utf-8")
            fixture_path.write_text(fixtures_body, encoding="utf-8")
            return self.arch_guard.scan_project_fixture_sources(row_path, fixture_path)

    def test_pinned_project_rows_with_sources_pass(self):
        rows = """
export const REQUIRED_PROJECT_ROWS = ["utility-types-project", "vite-vanilla-ts-app"];
export const COMPILE_CANARY_PROJECT_ROWS = ["type-challenges-assertions-tsc-clean"];
"""
        fixtures = """
tsz_project_fixture_sources() {
  case "$1" in
    utility-types-project)
      printf 'utility-types|repo|ref\\n'
      ;;
    type-challenges-assertions-tsc-clean)
      printf 'type-challenges|repo|ref\\n'
      printf 'type-challenges-solutions|repo|ref\\n'
      ;;
  esac
}
"""
        self.assertEqual(self._write_and_scan(rows, fixtures), [])

    def test_shared_project_row_definitions_with_sources_pass(self):
        rows = """
export const PROJECT_ROW_DEFINITIONS = [
  {
    name: "utility-types-project",
    benchmark_set: "required",
    guard_set: "required",
  },
  {
    name: "vite-vanilla-ts-app",
    benchmark_set: "required",
    guard_set: null,
  },
  {
    name: "type-challenges-project",
    benchmark_set: "canary",
    guard_set: "canary",
  },
];

export const REQUIRED_PROJECT_ROWS = PROJECT_ROW_DEFINITIONS
  .filter((row) => row.benchmark_set === "required")
  .map((row) => row.name);

export const COMPILE_CANARY_PROJECT_ROWS = PROJECT_ROW_DEFINITIONS
  .filter((row) => row.guard_set === "canary")
  .map((row) => row.name);
"""
        fixtures = """
tsz_project_fixture_sources() {
  case "$1" in
    utility-types-project)
      printf 'utility-types|repo|ref\\n'
      ;;
    type-challenges-project)
      printf 'type-challenges|repo|ref\\n'
      ;;
  esac
}
"""
        self.assertEqual(self._write_and_scan(rows, fixtures), [])

    def test_missing_source_metadata_is_reported(self):
        rows = """
export const REQUIRED_PROJECT_ROWS = ["utility-types-project"];
export const COMPILE_CANARY_PROJECT_ROWS = ["type-challenges-project"];
"""
        fixtures = """
tsz_project_fixture_sources() {
  case "$1" in
    utility-types-project)
      printf 'utility-types|repo|ref\\n'
      ;;
  esac
}
"""
        hits = self._write_and_scan(rows, fixtures)
        self.assertEqual(len(hits), 1)
        self.assertIn("missing fixture source metadata for type-challenges-project", hits[0])

    def test_stale_source_metadata_is_reported(self):
        rows = """
export const REQUIRED_PROJECT_ROWS = ["utility-types-project"];
export const COMPILE_CANARY_PROJECT_ROWS = [];
"""
        fixtures = """
tsz_project_fixture_sources() {
  case "$1" in
    utility-types-project)
      printf 'utility-types|repo|ref\\n'
      ;;
    removed-project)
      printf 'removed|repo|ref\\n'
      ;;
  esac
}
"""
        hits = self._write_and_scan(rows, fixtures)
        self.assertEqual(len(hits), 1)
        self.assertIn("stale fixture source metadata for removed-project", hits[0])

    def test_duplicate_source_metadata_case_is_reported(self):
        rows = """
export const REQUIRED_PROJECT_ROWS = ["utility-types-project"];
export const COMPILE_CANARY_PROJECT_ROWS = [];
"""
        fixtures = """
tsz_project_fixture_sources() {
  case "$1" in
    utility-types-project)
      printf 'utility-types|repo|ref\\n'
      ;;
    utility-types-project)
      printf 'utility-types|repo|ref\\n'
      ;;
  esac
}
"""
        hits = self._write_and_scan(rows, fixtures)
        self.assertEqual(len(hits), 1)
        self.assertIn("duplicate fixture source metadata for utility-types-project", hits[0])

    def test_empty_source_metadata_case_is_reported(self):
        rows = """
export const REQUIRED_PROJECT_ROWS = ["utility-types-project"];
export const COMPILE_CANARY_PROJECT_ROWS = [];
"""
        fixtures = """
tsz_project_fixture_sources() {
  case "$1" in
    utility-types-project)
      ;;
  esac
}
"""
        hits = self._write_and_scan(rows, fixtures)
        self.assertEqual(len(hits), 1)
        self.assertIn("empty fixture source metadata for utility-types-project", hits[0])

    def test_malformed_source_metadata_line_is_reported(self):
        rows = """
export const REQUIRED_PROJECT_ROWS = ["utility-types-project"];
export const COMPILE_CANARY_PROJECT_ROWS = ["type-challenges-project"];
"""
        fixtures = """
tsz_project_fixture_sources() {
  case "$1" in
    utility-types-project)
      printf 'utility-types|repo\\n'
      ;;
    type-challenges-project)
      printf 'type-challenges|repo|\\n'
      ;;
  esac
}
"""
        hits = self._write_and_scan(rows, fixtures)
        self.assertEqual(len(hits), 2)
        self.assertTrue(
            any("malformed fixture source metadata for utility-types-project" in hit for hit in hits),
            hits,
        )
        self.assertTrue(
            any("malformed fixture source metadata for type-challenges-project" in hit for hit in hits),
            hits,
        )

    def test_check_is_registered(self):
        names = [entry[0] for entry in self.arch_guard.PROJECT_FIXTURE_SOURCE_CHECKS]
        self.assertTrue(
            any("Track 1" in name for name in names),
            "Project fixture source check missing from PROJECT_FIXTURE_SOURCE_CHECKS",
        )

    def test_real_project_fixture_sources_cover_expected_rows(self):
        for name, row_path, fixture_path in self.arch_guard.PROJECT_FIXTURE_SOURCE_CHECKS:
            hits = self.arch_guard.scan_project_fixture_sources(row_path, fixture_path)
            self.assertEqual(hits, [], f"{name}: {hits[:5]}")


class ArchGuardProjectInclusionPolicyTests(unittest.TestCase):
    """Cover Track 1 project row inclusion-policy drift checks."""

    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _write_and_scan(self, rows_body: str, compile_body: str, bench_body: str):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            row_path = root / "project-rows.mjs"
            compile_path = root / "project-compile-guard.sh"
            bench_path = root / "bench-vs-tsgo.sh"
            row_path.write_text(rows_body, encoding="utf-8")
            compile_path.write_text(compile_body, encoding="utf-8")
            bench_path.write_text(bench_body, encoding="utf-8")
            return self.arch_guard.scan_project_inclusion_policy(
                row_path,
                compile_path,
                bench_path,
            )

    def test_matching_compile_and_benchmark_inclusions_pass(self):
        rows = """
export const REQUIRED_PROJECT_ROWS = ["utility-types-project", "vite-vanilla-ts-app"];
export const COMPILE_CANARY_PROJECT_ROWS = ["type-challenges-assertion-candidates"];
"""
        compile_guard = """
if should_check_project "utility-types-project"; then :; fi
if should_check_project "vite-vanilla-ts-app"; then :; fi
if should_check_project "type-challenges-assertion-candidates"; then :; fi
"""
        bench = """
run_isolated "utility-types-project" run_utility_types_project_benchmarks
run_isolated "vite-vanilla-ts-app" run_vite_app_project_benchmarks
"""
        self.assertEqual(self._write_and_scan(rows, compile_guard, bench), [])

    def test_shared_project_row_definitions_with_dynamic_loops_pass(self):
        rows = """
export const PROJECT_ROW_DEFINITIONS = [
  {
    name: "utility-types-project",
    benchmark_set: "required",
    guard_set: "required",
  },
  {
    name: "zod-project",
    benchmark_set: "canary",
    guard_set: "canary",
  },
  {
    name: "vite-vanilla-ts-app",
    benchmark_set: "required",
    guard_set: null,
  },
];

export const REQUIRED_PROJECT_ROWS = PROJECT_ROW_DEFINITIONS
  .filter((row) => row.benchmark_set === "required")
  .map((row) => row.name);

export const COMPILE_CANARY_PROJECT_ROWS = PROJECT_ROW_DEFINITIONS
  .filter((row) => row.guard_set === "canary")
  .map((row) => row.name);
"""
        compile_guard = """
for name in "${TSZ_COMPILE_GUARD_REQUIRED_ROWS[@]}"; do
  if should_check_project "$name"; then :; fi
done
if should_check_project "vite-vanilla-ts-app"; then :; fi
for name in "${TSZ_COMPILE_GUARD_CANARY_ROWS[@]}"; do
  if should_check_project "$name"; then :; fi
done
"""
        bench = """
run_isolated "utility-types-project" run_utility_types_project_benchmarks
run_isolated "zod-project" run_zod_project_benchmarks
run_isolated "vite-vanilla-ts-app" run_vite_app_project_benchmarks
"""
        self.assertEqual(self._write_and_scan(rows, compile_guard, bench), [])

    def test_missing_compile_guard_inclusion_is_reported(self):
        rows = """
export const REQUIRED_PROJECT_ROWS = ["utility-types-project"];
export const COMPILE_CANARY_PROJECT_ROWS = ["zod-project"];
"""
        compile_guard = 'if should_check_project "utility-types-project"; then :; fi'
        bench = """
run_isolated "utility-types-project" run_utility_types_project_benchmarks
run_isolated "zod-project" run_zod_project_benchmarks
"""
        hits = self._write_and_scan(rows, compile_guard, bench)
        self.assertEqual(len(hits), 1)
        self.assertIn("missing project compile guard inclusion for zod-project", hits[0])

    def test_stale_compile_guard_inclusion_is_reported(self):
        rows = """
export const REQUIRED_PROJECT_ROWS = ["utility-types-project"];
export const COMPILE_CANARY_PROJECT_ROWS = [];
"""
        compile_guard = """
if should_check_project "utility-types-project"; then :; fi
if should_check_project "removed-project"; then :; fi
"""
        bench = 'run_isolated "utility-types-project" run_utility_types_project_benchmarks'
        hits = self._write_and_scan(rows, compile_guard, bench)
        self.assertEqual(len(hits), 1)
        self.assertIn("stale project compile guard inclusion for removed-project", hits[0])

    def test_missing_benchmark_inclusion_is_reported(self):
        rows = """
export const REQUIRED_PROJECT_ROWS = ["utility-types-project", "type-fest-project"];
export const COMPILE_CANARY_PROJECT_ROWS = [];
"""
        compile_guard = """
if should_check_project "utility-types-project"; then :; fi
if should_check_project "type-fest-project"; then :; fi
"""
        bench = 'run_isolated "utility-types-project" run_utility_types_project_benchmarks'
        hits = self._write_and_scan(rows, compile_guard, bench)
        self.assertEqual(len(hits), 1)
        self.assertIn("missing project benchmark inclusion for type-fest-project", hits[0])

    def test_compile_guard_only_rows_do_not_require_benchmark_inclusion(self):
        rows = """
export const REQUIRED_PROJECT_ROWS = [];
export const COMPILE_CANARY_PROJECT_ROWS = ["type-challenges-assertions-tsc-clean"];
"""
        compile_guard = 'if should_check_project "type-challenges-assertions-tsc-clean"; then :; fi'
        bench = ""
        self.assertEqual(self._write_and_scan(rows, compile_guard, bench), [])

    def test_benchmark_only_rows_do_not_require_compile_guard_inclusion(self):
        rows = """
export const REQUIRED_PROJECT_ROWS = ["large-ts-repo", "nextjs"];
export const COMPILE_CANARY_PROJECT_ROWS = [];
"""
        compile_guard = ""
        bench = """
run_isolated "large-ts-repo" run_large_ts_repo_benchmarks
run_isolated "nextjs" run_nextjs_benchmarks
"""
        self.assertEqual(self._write_and_scan(rows, compile_guard, bench), [])

    def test_check_is_registered(self):
        names = [entry[0] for entry in self.arch_guard.PROJECT_INCLUSION_POLICY_CHECKS]
        self.assertTrue(
            any("Track 1" in name for name in names),
            "Project inclusion policy check missing from PROJECT_INCLUSION_POLICY_CHECKS",
        )

    def test_real_project_inclusion_policy_matches_manifest(self):
        for name, row_path, compile_path, bench_path in self.arch_guard.PROJECT_INCLUSION_POLICY_CHECKS:
            hits = self.arch_guard.scan_project_inclusion_policy(
                row_path,
                compile_path,
                bench_path,
            )
            self.assertEqual(hits, [], f"{name}: {hits[:5]}")


class ArchGuardProjectConfigWriterTests(unittest.TestCase):
    """Cover Track 1 shared project config writer drift checks."""

    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _write_and_scan(self, fixture_body: str, compile_body: str, bench_body: str):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            fixture_path = root / "project-fixtures.sh"
            compile_path = root / "project-compile-guard.sh"
            bench_path = root / "bench-vs-tsgo.sh"
            fixture_path.write_text(fixture_body, encoding="utf-8")
            compile_path.write_text(compile_body, encoding="utf-8")
            bench_path.write_text(bench_body, encoding="utf-8")

            original = self.arch_guard.PROJECT_CONFIG_WRITERS
            try:
                self.arch_guard.PROJECT_CONFIG_WRITERS = {
                    "utility-types-project": "tsz_write_utility_types_config",
                    "nextjs": "tsz_write_nextjs_config",
                }
                return self.arch_guard.scan_project_config_writers(
                    fixture_path,
                    compile_path,
                    bench_path,
                )
            finally:
                self.arch_guard.PROJECT_CONFIG_WRITERS = original

    def test_shared_config_writer_usage_passes(self):
        fixtures = """
tsz_write_utility_types_config() { :; }
tsz_write_nextjs_config() { :; }
"""
        compile_guard = "tsz_write_utility_types_config \"$out\""
        bench = """
tsz_write_utility_types_config "$out"
tsz_write_nextjs_config "$out"
"""
        self.assertEqual(self._write_and_scan(fixtures, compile_guard, bench), [])

    def test_missing_shared_writer_is_reported(self):
        fixtures = 'tsz_write_nextjs_config() { :; }'
        compile_guard = "tsz_write_utility_types_config \"$out\""
        bench = """
tsz_write_utility_types_config "$out"
tsz_write_nextjs_config "$out"
"""
        hits = self._write_and_scan(fixtures, compile_guard, bench)
        self.assertEqual(len(hits), 1)
        self.assertIn("missing shared config writer tsz_write_utility_types_config", hits[0])

    def test_compile_guard_missing_writer_use_is_reported(self):
        fixtures = """
tsz_write_utility_types_config() { :; }
tsz_write_nextjs_config() { :; }
"""
        compile_guard = ""
        bench = """
tsz_write_utility_types_config "$out"
tsz_write_nextjs_config "$out"
"""
        hits = self._write_and_scan(fixtures, compile_guard, bench)
        self.assertEqual(len(hits), 1)
        self.assertIn("utility-types-project does not use shared config writer", hits[0])

    def test_benchmark_missing_writer_use_is_reported(self):
        fixtures = """
tsz_write_utility_types_config() { :; }
tsz_write_nextjs_config() { :; }
"""
        compile_guard = "tsz_write_utility_types_config \"$out\""
        bench = 'tsz_write_nextjs_config "$out"'
        hits = self._write_and_scan(fixtures, compile_guard, bench)
        self.assertEqual(len(hits), 1)
        self.assertIn("utility-types-project does not use shared config writer", hits[0])

    def test_benchmark_only_row_does_not_require_compile_guard_writer_use(self):
        fixtures = """
tsz_write_utility_types_config() { :; }
tsz_write_nextjs_config() { :; }
"""
        compile_guard = 'tsz_write_utility_types_config "$out"'
        bench = """
tsz_write_utility_types_config "$out"
tsz_write_nextjs_config "$out"
"""
        self.assertEqual(self._write_and_scan(fixtures, compile_guard, bench), [])

    def test_check_is_registered(self):
        names = [entry[0] for entry in self.arch_guard.PROJECT_CONFIG_WRITER_CHECKS]
        self.assertTrue(
            any("Track 1" in name for name in names),
            "Project config writer check missing from PROJECT_CONFIG_WRITER_CHECKS",
        )

    def test_real_project_config_writers_are_shared(self):
        for name, fixture_path, compile_path, bench_path in self.arch_guard.PROJECT_CONFIG_WRITER_CHECKS:
            hits = self.arch_guard.scan_project_config_writers(
                fixture_path,
                compile_path,
                bench_path,
            )
            self.assertEqual(hits, [], f"{name}: {hits[:5]}")


class ArchGuardRegexLineCountTests(unittest.TestCase):
    """Cover Track 10 count ratchets in `REGEX_LINE_COUNT_CHECKS`.

    These checks make post-check fingerprint rewrites, checker and emitter
    source-text snippet decisions, and rendered-type string decisions visible
    in the shared architecture guard output.
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

    def _check_by_name(self, needle: str):
        for entry in self.arch_guard.REGEX_LINE_COUNT_CHECKS:
            name, _search_roots, pattern, max_lines = entry
            if needle in name:
                return pattern, max_lines
        self.fail(f"missing REGEX_LINE_COUNT_CHECKS entry containing {needle!r}")

    def test_flags_rewrite_fingerprint_function_defs(self):
        pattern, _max_lines = self._check_by_name("rewrite_*_fingerprints")
        root = self._make_tree(
            {
                "crates/tsz-checker/src/source_file.rs": (
                    "fn rewrite_alpha_fingerprints(&mut self) {}\n"
                    "self.rewrite_beta_fingerprints();\n"
                ),
            }
        )
        hits = self.arch_guard.scan_regex_line_count([root], pattern, 0)
        self.assertEqual(len(hits), 2, f"unexpected hits: {hits!r}")
        self.assertIn("source_file.rs:1", hits[0])
        self.assertIn("total matching lines: 1", hits[1])

    def test_flags_source_text_contains_lines(self):
        pattern, _max_lines = self._check_by_name("source_text.contains")
        root = self._make_tree(
            {
                "crates/tsz-checker/src/source_decision.rs": (
                    'if source_text.contains("fixture shape") {}\n'
                ),
            }
        )
        hits = self.arch_guard.scan_regex_line_count([root], pattern, 0)
        self.assertEqual(len(hits), 2, f"unexpected hits: {hits!r}")
        self.assertIn("source_decision.rs:1", hits[0])

    def test_flags_emitter_source_text_contains_lines(self):
        pattern, _max_lines = self._check_by_name("Emitter boundary")
        root = self._make_tree(
            {
                "crates/tsz-emitter/src/recovery.rs": (
                    'if source_text.contains("malformed emit shape") {}\n'
                ),
            }
        )
        hits = self.arch_guard.scan_regex_line_count([root], pattern, 0)
        self.assertEqual(len(hits), 2, f"unexpected hits: {hits!r}")
        self.assertIn("recovery.rs:1", hits[0])

    def test_flags_file_name_and_path_substring_decisions(self):
        pattern, _max_lines = self._check_by_name("file-name/path")
        root = self._make_tree(
            {
                "crates/tsz-checker/src/file_decision.rs": (
                    'if file_name.contains("node_modules") {}\n'
                    "if source_file.file_name.contains(\"node_modules\") {}\n"
                    'if !source_path.contains("/node_modules/") {}\n'
                ),
            }
        )
        hits = self.arch_guard.scan_regex_line_count([root], pattern, 0)
        self.assertEqual(len(hits), 4, f"unexpected hits: {hits!r}")
        self.assertIn("file_decision.rs:1", hits[0])
        self.assertIn("file_decision.rs:2", hits[1])
        self.assertIn("file_decision.rs:3", hits[2])

    def test_flags_rendered_type_string_decisions(self):
        pattern, _max_lines = self._check_by_name("rendered type strings")
        root = self._make_tree(
            {
                "crates/tsz-checker/src/rendered.rs": (
                    'if self.format_type(ty).contains("Readonly<") {}\n'
                    'if matches!(self.format_type(base).as_str(), "Element") {}\n'
                    "let display = self.format_type(ty);\n"
                ),
            }
        )
        hits = self.arch_guard.scan_regex_line_count([root], pattern, 0)
        self.assertEqual(len(hits), 3, f"unexpected hits: {hits!r}")
        self.assertIn("rendered.rs:1", hits[0])
        self.assertIn("rendered.rs:2", hits[1])

    def test_flags_raw_diagnostic_assignability_predicates(self):
        pattern, _max_lines = self._check_by_name(
            "raw diagnostic assignability predicates"
        )
        root = self._make_tree(
            {
                "crates/tsz-checker/src/error_reporter/diagnostic.rs": (
                    "if self.is_assignable_to(source, target) {}\n"
                    "if self.ctx.types.is_assignable_to(source, target) {}\n"
                    "if self.is_assignable_to_with_env(source, target) {}\n"
                ),
            }
        )
        hits = self.arch_guard.scan_regex_line_count([root], pattern, 0)
        self.assertEqual(len(hits), 4, f"unexpected hits: {hits!r}")
        self.assertIn("diagnostic.rs:1", hits[0])
        self.assertIn("diagnostic.rs:2", hits[1])
        self.assertIn("diagnostic.rs:3", hits[2])

    def test_flags_diagnostic_local_relation_request_constructors(self):
        pattern, _max_lines = self._check_by_name("diagnostic-local RelationRequest")
        root = self._make_tree(
            {
                "crates/tsz-checker/src/error_reporter/diagnostic.rs": (
                    "let request = RelationRequest::assign(source, target);\n"
                    "let request = RelationRequest::call_arg(source, target);\n"
                    "let outcome = self.assign_relation_outcome(source, target);\n"
                ),
            }
        )
        hits = self.arch_guard.scan_regex_line_count([root], pattern, 0)
        self.assertEqual(len(hits), 3, f"unexpected hits: {hits!r}")
        self.assertIn("diagnostic.rs:1", hits[0])
        self.assertIn("diagnostic.rs:2", hits[1])
        self.assertIn("total matching lines: 2", hits[2])

    def test_flags_legacy_relation_bridge_call_surface(self):
        pattern, _max_lines = self._check_by_name("#8207")
        root = self._make_tree(
            {
                "crates/tsz-solver/src/types.rs": (
                    "fn from_legacy_u8(raw: u8) -> CachedAnyMode { todo!() }\n"
                    "let key = RelationCacheKey::subtype(source, target, flags);\n"
                    "let flags = RelationFlags::from_bits_truncate(raw);\n"
                    "let mode = CachedAnyMode::from_legacy_u8(raw);\n"
                ),
            }
        )
        hits = self.arch_guard.scan_regex_line_count([root], pattern, 0)
        self.assertEqual(len(hits), 5, f"unexpected hits: {hits!r}")
        self.assertIn("types.rs:1", hits[0])
        self.assertIn("types.rs:4", hits[3])
        self.assertIn("total matching lines: 4", hits[4])

    def test_legacy_relation_bridge_guard_ignores_text_only_mentions(self):
        pattern, _max_lines = self._check_by_name("#8207")
        root = self._make_tree(
            {
                "crates/tsz-solver/src/types.rs": (
                    '// RelationCacheKey::subtype(source, target, flags)\n'
                    'let message = "RelationCacheKey::subtype(source, target, flags)";\n'
                    'let helper = "from_legacy_u8(raw)";\n'
                    'let bare_name = "CachedAnyMode::from_legacy_u8";\n'
                ),
            }
        )
        hits = self.arch_guard.scan_regex_line_count([root], pattern, 0)
        self.assertEqual(hits, [], f"unexpected hits: {hits!r}")

    def test_flags_root_solver_wildcard_compat_reexports(self):
        pattern, _max_lines = self._check_by_name("#8204")
        root = self._make_tree(
            {
                "crates/tsz-solver/src/lib.rs": (
                    "pub use evaluation::evaluate::*;\n"
                    "pub mod query {\n"
                    "    pub use crate::visitors::visitor::*;\n"
                    "}\n"
                    "// pub use operations::*;\n"
                ),
            }
        )
        hits = self.arch_guard.scan_regex_line_count([root], pattern, 0)
        self.assertEqual(len(hits), 2, f"unexpected hits: {hits!r}")
        self.assertIn("lib.rs:1", hits[0])

    def test_scan_regex_line_count_accepts_file_roots(self):
        pattern, _max_lines = self._check_by_name(
            "raw diagnostic assignability predicates"
        )
        root = self._make_tree(
            {
                "crates/tsz-checker/src/assignability/assignability_diagnostics.rs": (
                    "if self.is_assignable_to(source, target) {}\n"
                ),
            }
        )
        file_root = (
            root
            / "crates"
            / "tsz-checker"
            / "src"
            / "assignability"
            / "assignability_diagnostics.rs"
        )
        hits = self.arch_guard.scan_regex_line_count([file_root], pattern, 0)
        self.assertEqual(len(hits), 2, f"unexpected hits: {hits!r}")
        self.assertIn("assignability_diagnostics.rs:1", hits[0])

    def test_excludes_tests_and_comment_lines(self):
        pattern, _max_lines = self._check_by_name("source_text.contains")
        root = self._make_tree(
            {
                "crates/tsz-checker/src/foo_tests.rs": (
                    'if source_text.contains("test") {}\n'
                ),
                "crates/tsz-checker/tests/integration.rs": (
                    'if source_text.contains("test") {}\n'
                ),
                "crates/tsz-checker/src/commented.rs": (
                    '// if source_text.contains("comment") {}\n'
                ),
            }
        )
        hits = self.arch_guard.scan_regex_line_count([root], pattern, 0)
        self.assertEqual(hits, [], f"unexpected hits: {hits!r}")

    def test_passes_when_at_or_under_cap(self):
        pattern, _max_lines = self._check_by_name("source_text.contains")
        root = self._make_tree(
            {
                "crates/tsz-checker/src/a.rs": 'if source_text.contains("a") {}\n',
                "crates/tsz-checker/src/b.rs": 'if source_text.contains("b") {}\n',
            }
        )
        self.assertEqual(self.arch_guard.scan_regex_line_count([root], pattern, 2), [])
        self.assertEqual(self.arch_guard.scan_regex_line_count([root], pattern, 3), [])

    def test_track10_checks_are_registered(self):
        names = [entry[0] for entry in self.arch_guard.REGEX_LINE_COUNT_CHECKS]
        self.assertTrue(any("post-check" in name for name in names))
        self.assertTrue(any("source_text.contains" in name for name in names))
        self.assertTrue(any("Emitter boundary" in name for name in names))
        self.assertTrue(any("file-name/path" in name for name in names))
        self.assertTrue(any("rendered type strings" in name for name in names))
        self.assertTrue(any("#8227" in name for name in names))
        self.assertTrue(any("diagnostic-local RelationRequest" in name for name in names))
        self.assertTrue(any("#8207" in name for name in names))
        self.assertTrue(any("#8204" in name for name in names))

    def test_real_counts_pass_at_pinned_caps(self):
        """The pinned caps must match the live count (no off-by-one)."""
        for entry in self.arch_guard.REGEX_LINE_COUNT_CHECKS:
            name, search_roots, pattern, max_lines = entry
            hits = self.arch_guard.scan_regex_line_count(
                search_roots, pattern, max_lines
            )
            self.assertEqual(
                hits,
                [],
                f"{name}: cap is too tight — guard fires at the live count.",
            )


class ArchGuardVisitedCloneTests(unittest.TestCase):
    """Cover Track 10 branch-local `visited.clone()` traversal guardrails."""

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

    def _registered_check(self):
        for entry in self.arch_guard.BRANCH_LOCAL_VISITED_CLONE_CHECKS:
            name, search_roots, allowlist = entry
            if "visited.clone()" in name:
                return name, search_roots, allowlist
        self.fail("visited.clone() performance guard is missing")

    def test_flags_new_branch_local_clone_site(self):
        root = self._make_tree(
            {
                "crates/tsz-checker/src/flow/new_predicate.rs": (
                    "fn walk(visited: Vec<u32>) {\n"
                    "    let mut branch_visited = visited.clone();\n"
                    "}\n"
                ),
            }
        )
        hits = self.arch_guard.scan_branch_local_visited_clones([root], ())
        self.assertEqual(len(hits), 1, f"unexpected hits: {hits!r}")
        self.assertIn("new_predicate.rs:2", hits[0])
        self.assertIn("memoized DP", hits[0])

    def test_allows_pinned_site_by_file_and_statement_text(self):
        root = self._make_tree(
            {
                "crates/tsz-checker/src/flow/control_flow/typeof_exclusions.rs": (
                    "fn walk(visited: Vec<u32>) {\n"
                    "\n"
                    "    let mut branch_visited = visited.clone();\n"
                    "}\n"
                ),
            }
        )
        allowlist = (
            (
                "crates/tsz-checker/src/flow/control_flow/typeof_exclusions.rs",
                "let mut branch_visited = visited.clone();",
            ),
        )
        hits = self.arch_guard.scan_branch_local_visited_clones([root], allowlist)
        self.assertEqual(hits, [], f"unexpected hits: {hits!r}")

    def test_duplicate_allowed_statement_still_flags_extra_site(self):
        root = self._make_tree(
            {
                "crates/tsz-checker/src/flow/control_flow/typeof_exclusions.rs": (
                    "fn walk(visited: Vec<u32>) {\n"
                    "    let mut branch_visited = visited.clone();\n"
                    "    let mut branch_visited = visited.clone();\n"
                    "}\n"
                ),
            }
        )
        allowlist = (
            (
                "crates/tsz-checker/src/flow/control_flow/typeof_exclusions.rs",
                "let mut branch_visited = visited.clone();",
            ),
        )
        hits = self.arch_guard.scan_branch_local_visited_clones([root], allowlist)
        self.assertEqual(len(hits), 1, f"unexpected hits: {hits!r}")
        self.assertIn("typeof_exclusions.rs:3", hits[0])

    def test_excludes_tests_and_comment_lines(self):
        root = self._make_tree(
            {
                "crates/tsz-checker/src/foo_tests.rs": (
                    "let mut branch_visited = visited.clone();\n"
                ),
                "crates/tsz-checker/tests/integration.rs": (
                    "let mut branch_visited = visited.clone();\n"
                ),
                "crates/tsz-checker/src/commented.rs": (
                    "// let mut branch_visited = visited.clone();\n"
                ),
            }
        )
        hits = self.arch_guard.scan_branch_local_visited_clones([root], ())
        self.assertEqual(hits, [], f"unexpected hits: {hits!r}")

    def test_registered_real_sites_pass(self):
        _name, search_roots, allowlist = self._registered_check()
        hits = self.arch_guard.scan_branch_local_visited_clones(
            search_roots, allowlist
        )
        self.assertEqual(hits, [], f"live visited.clone() allowlist is stale: {hits!r}")


class ArchGuardPolicyTomlTests(unittest.TestCase):
    """Verify that CHECKS and MANIFEST_CHECKS are loaded from the TOML policy file.

    These tests exercise the loader functions directly with controlled TOML input
    so engine correctness is validated independently of the live policy data.
    """

    def setUp(self):
        self.arch_guard = load_arch_guard_module()


    def _load_from_toml(self, toml_text: str, loader):
        with tempfile.TemporaryDirectory() as tmp:
            path = pathlib.Path(tmp) / "test_policy.toml"
            path.write_text(toml_text, encoding="utf-8")
            return loader(path)

    def _load_checks(self, toml_text: str):
        return self._load_from_toml(toml_text, self.arch_guard._load_pattern_checks)

    def _load_manifest(self, toml_text: str):
        return self._load_from_toml(toml_text, self.arch_guard._load_manifest_checks)


    def test_live_checks_matches_expected_count(self):
        """CHECKS must be loaded from TOML and have the expected entry count."""
        self.assertEqual(len(self.arch_guard.CHECKS), 31)

    def test_live_manifest_checks_matches_expected_count(self):
        """MANIFEST_CHECKS must be loaded from TOML and have the expected entry count."""
        self.assertEqual(len(self.arch_guard.MANIFEST_CHECKS), 3)

    def test_live_policy_file_exists(self):
        policy_path = self.arch_guard.POLICY_PATH
        self.assertTrue(
            policy_path.exists(),
            f"Policy file missing: {policy_path}",
        )

    def test_checks_are_tuples_with_correct_shape(self):
        """Each CHECKS entry must be (name, base_path, compiled_pattern, excludes_dict)."""
        for name, base, pattern, excludes in self.arch_guard.CHECKS:
            self.assertIsInstance(name, str)
            self.assertIsInstance(base, pathlib.Path)
            self.assertIsInstance(pattern, type(self.arch_guard.re.compile("")))
            self.assertIsInstance(excludes, dict)

    def test_manifest_checks_are_tuples_with_correct_shape(self):
        """Each MANIFEST_CHECKS entry must be (name, file_path, compiled_pattern)."""
        for name, file_path, pattern in self.arch_guard.MANIFEST_CHECKS:
            self.assertIsInstance(name, str)
            self.assertIsInstance(file_path, pathlib.Path)
            self.assertIsInstance(pattern, type(self.arch_guard.re.compile("")))


    def test_minimal_pattern_check_loads(self):
        """A pattern_checks entry with only required fields loads correctly."""
        checks = self._load_checks("""
[[pattern_checks]]
name = "test rule"
base = "crates"
pattern = '\\bfoo\\b'
""")
        self.assertEqual(len(checks), 1)
        name, base, pattern, excludes = checks[0]
        self.assertEqual(name, "test rule")
        self.assertEqual(excludes, {})
        self.assertIsNotNone(pattern.search("foo"))
        self.assertIsNone(pattern.search("foobar"))

    def test_exclude_dirs_parsed_as_set(self):
        checks = self._load_checks("""
[[pattern_checks]]
name = "r"
base = "crates"
pattern = '\\bx\\b'
exclude_dirs = ["tests", "benches"]
""")
        _, _, _, excludes = checks[0]
        self.assertIn("exclude_dirs", excludes)
        self.assertIsInstance(excludes["exclude_dirs"], set)
        self.assertEqual(excludes["exclude_dirs"], {"tests", "benches"})

    def test_exclude_files_parsed_as_set(self):
        checks = self._load_checks("""
[[pattern_checks]]
name = "r"
base = "crates"
pattern = '\\bx\\b'
exclude_files = ["crates/foo/src/bar.rs", "crates/baz/src/qux.rs"]
""")
        _, _, _, excludes = checks[0]
        self.assertIn("exclude_files", excludes)
        self.assertIsInstance(excludes["exclude_files"], set)
        self.assertEqual(
            excludes["exclude_files"],
            {"crates/foo/src/bar.rs", "crates/baz/src/qux.rs"},
        )

    def test_exclude_test_files_flag(self):
        checks_on = self._load_checks("""
[[pattern_checks]]
name = "r"
base = "crates"
pattern = '\\bx\\b'
exclude_test_files = true
""")
        _, _, _, excludes = checks_on[0]
        self.assertTrue(excludes.get("exclude_test_files"))

        checks_off = self._load_checks("""
[[pattern_checks]]
name = "r"
base = "crates"
pattern = '\\bx\\b'
exclude_test_files = false
""")
        _, _, _, excludes_off = checks_off[0]
        self.assertNotIn("exclude_test_files", excludes_off)

    def test_ignore_comment_lines_flag(self):
        checks_on = self._load_checks("""
[[pattern_checks]]
name = "r"
base = "crates"
pattern = '\\bx\\b'
ignore_comment_lines = true
""")
        _, _, _, excludes = checks_on[0]
        self.assertTrue(excludes.get("ignore_comment_lines"))

    def test_multiple_pattern_checks_in_order(self):
        checks = self._load_checks("""
[[pattern_checks]]
name = "first"
base = "crates"
pattern = '\\bfirst\\b'

[[pattern_checks]]
name = "second"
base = "src"
pattern = '\\bsecond\\b'
""")
        self.assertEqual(len(checks), 2)
        self.assertEqual(checks[0][0], "first")
        self.assertEqual(checks[1][0], "second")

    def test_empty_exclude_dirs_not_added_to_excludes(self):
        """An absent or empty exclude_dirs must not appear in the excludes dict."""
        checks = self._load_checks("""
[[pattern_checks]]
name = "r"
base = "crates"
pattern = '\\bx\\b'
""")
        _, _, _, excludes = checks[0]
        self.assertNotIn("exclude_dirs", excludes)


    def test_loaded_pattern_works_with_find_matches(self):
        """Patterns loaded from TOML produce the same find_matches results
        as the equivalent Python re.compile call."""
        checks = self._load_checks("""
[[pattern_checks]]
name = "boundary rule"
base = "crates"
pattern = '\\bCompatChecker::new\\b'
exclude_dirs = ["query_boundaries", "tests"]
ignore_comment_lines = true
""")
        name, base, pattern, excludes = checks[0]
        hit_text = "let c = CompatChecker::new(db);"
        comment_text = "// CompatChecker::new"
        excluded_path = "crates/foo/src/query_boundaries/bar.rs"
        normal_path = "crates/foo/src/checker.rs"

        self.assertEqual(
            self.arch_guard.find_matches(hit_text, pattern, normal_path, excludes),
            [1],
            "should flag a real use in a non-excluded file",
        )
        self.assertEqual(
            self.arch_guard.find_matches(comment_text, pattern, normal_path, excludes),
            [],
            "should ignore a line starting with //",
        )
        self.assertEqual(
            self.arch_guard.find_matches(hit_text, pattern, excluded_path, excludes),
            [],
            "should skip files in exclude_dirs",
        )

    def test_exclude_test_file_behaviour_through_loader(self):
        """exclude_test_files=true must skip *_tests.rs files."""
        checks = self._load_checks("""
[[pattern_checks]]
name = "no unwrap"
base = "crates"
pattern = '\\.unwrap\\(\\)'
exclude_test_files = true
ignore_comment_lines = true
""")
        _, _, pattern, excludes = checks[0]
        hit = "let x = foo().unwrap();"
        test_file = "crates/tsz-checker/src/foo_tests.rs"
        prod_file = "crates/tsz-checker/src/foo.rs"

        self.assertEqual(
            self.arch_guard.find_matches(hit, pattern, test_file, excludes),
            [],
            "should skip *_tests.rs files",
        )
        self.assertEqual(
            self.arch_guard.find_matches(hit, pattern, prod_file, excludes),
            [1],
            "should flag production files",
        )


    def test_manifest_check_pattern_and_multiline(self):
        """manifest_checks patterns must be compiled with MULTILINE so ^ matches
        at each line start within a Cargo.toml file."""
        manifest_checks = self._load_manifest("""
[[manifest_checks]]
name = "no bad dep"
file = "crates/tsz-emitter/Cargo.toml"
pattern = '^\\s*bad-crate\\s*='
""")
        self.assertEqual(len(manifest_checks), 1)
        name, file_path, pattern = manifest_checks[0]
        self.assertEqual(name, "no bad dep")
        # Pattern must match at line start within a multi-line string.
        toml_content = "[dependencies]\nbad-crate = \"1.0\"\nother = \"2\"\n"
        self.assertIsNotNone(pattern.search(toml_content))
        self.assertIsNone(pattern.search("[dependencies]\n# bad-crate = \"1.0\"\n"))

    def test_multiple_manifest_checks_in_order(self):
        checks = self._load_manifest("""
[[manifest_checks]]
name = "first"
file = "crates/a/Cargo.toml"
pattern = '^\\s*dep-a\\s*='

[[manifest_checks]]
name = "second"
file = "crates/b/Cargo.toml"
pattern = '^\\s*dep-b\\s*='
""")
        self.assertEqual(len(checks), 2)
        self.assertEqual(checks[0][0], "first")
        self.assertEqual(checks[1][0], "second")


    def test_new_rule_added_via_toml_is_immediately_active(self):
        """Adding a new [[pattern_checks]] entry to the policy TOML must make
        the rule active in CHECKS without any engine code changes.  This test
        demonstrates the acceptance criterion from issue #8287."""
        new_rule_toml = """
[[pattern_checks]]
name = "Custom rule: no forbidden_call() in production code"
base = "crates"
pattern = '\\.forbidden_call\\(\\)'
exclude_dirs = ["tests"]
ignore_comment_lines = true
"""
        checks = self._load_checks(new_rule_toml)
        self.assertEqual(len(checks), 1)
        name, base, pattern, excludes = checks[0]
        self.assertEqual(name, "Custom rule: no forbidden_call() in production code")
        self.assertEqual(excludes.get("exclude_dirs"), {"tests"})
        self.assertTrue(excludes.get("ignore_comment_lines"))

        # Verify the pattern matches what it should.
        hit = "    self.forbidden_call();"
        self.assertEqual(
            self.arch_guard.find_matches(
                hit, pattern, "crates/foo/src/lib.rs", excludes
            ),
            [1],
        )
        # And is ignored in tests/ dirs.
        self.assertEqual(
            self.arch_guard.find_matches(
                hit, pattern, "crates/foo/tests/integration.rs", excludes
            ),
            [],
        )

    def test_new_manifest_rule_added_via_toml(self):
        """Adding a new [[manifest_checks]] entry must work without engine changes."""
        new_rule = """
[[manifest_checks]]
name = "No forbidden-dep"
file = "crates/tsz-checker/Cargo.toml"
pattern = '^\\s*forbidden-dep\\s*='
"""
        checks = self._load_manifest(new_rule)
        self.assertEqual(len(checks), 1)
        name, _, pattern = checks[0]
        self.assertEqual(name, "No forbidden-dep")
        self.assertIsNone(pattern.search("[dev-dependencies]\nallowed = \"1\"\n"))
        self.assertIsNotNone(pattern.search("forbidden-dep = \"0.1\"\n"))


if __name__ == "__main__":
    unittest.main()
