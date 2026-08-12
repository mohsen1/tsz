import json

from test_arch_guard_support import ROOT, load_arch_guard_module, pathlib, tempfile, unittest
from test_arch_guard_counts import *
from test_arch_guard_lsp import *
from test_arch_guard_project import *
from test_arch_guard_policy import *


class ArchGuardJsonReportTests(unittest.TestCase):
    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def test_json_payload_includes_boolean_ok_status(self):
        git_context = {
            "repo_root": "/repo",
            "head": "abc123",
            "branch": "codex/example",
            "upstream": "origin/main",
            "dirty": False,
            "dirty_path_count": 0,
        }
        self.assertEqual(
            self.arch_guard.build_json_payload([], 0, git_context=git_context),
            {
                "ok": True,
                "status": "passed",
                "arch_guard_status": "passed",
                "git_context": git_context,
                "total_hits": 0,
                "failure_count": 0,
                "failed_hit_count": 0,
                "failures": [],
            },
        )
        self.assertEqual(
            self.arch_guard.build_json_payload(
                [("Rule", ["src/lib.rs:1", "src/lib.rs:2"])],
                2,
                git_context=git_context,
            ),
            {
                "ok": False,
                "status": "failed",
                "arch_guard_status": "failed",
                "git_context": git_context,
                "total_hits": 2,
                "failure_count": 1,
                "failed_hit_count": 2,
                "failures": [
                    {"name": "Rule", "hits": ["src/lib.rs:1", "src/lib.rs:2"]}
                ],
            },
        )

    def test_write_json_report_is_atomic_and_stable(self):
        payload = {
            "ok": True,
            "total_hits": 0,
            "status": "passed",
            "arch_guard_status": "passed",
            "failure_count": 0,
            "failed_hit_count": 0,
            "failures": [],
        }
        with tempfile.TemporaryDirectory() as tmp:
            report_path = pathlib.Path(tmp) / "nested" / "arch_guard.json"
            self.arch_guard.write_json_report(report_path, payload)

            self.assertEqual(json.loads(report_path.read_text(encoding="utf-8")), payload)
            self.assertTrue(report_path.read_text(encoding="utf-8").endswith("\n"))
            self.assertFalse(report_path.with_name(".arch_guard.json.tmp").exists())


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


class ArchGuardCheckerSemanticProofBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _semantic_proof_check(self):
        for name, _base, pattern, excludes in self.arch_guard.CHECKS:
            if name == "Checker boundary: semantic proof helpers stay behind query boundaries (#9673)":
                return pattern, excludes
        self.fail("checker semantic proof boundary check is missing from CHECKS")

    def test_rule_exists(self):
        self._semantic_proof_check()

    def test_rule_flags_checker_local_conditional_and_instantiation_proofs(self):
        pattern, excludes = self._semantic_proof_check()
        text = "\n".join(
            [
                "let components = query::full_conditional_type_components(db, ty);",
                "let ty = query_boundaries::common::instantiate_generic(db, base, args);",
            ]
        )
        hits = self.arch_guard.find_matches(
            text,
            pattern,
            "crates/tsz-checker/src/checkers/generic_checker/boolean_probe_constraints.rs",
            excludes,
        )
        self.assertEqual(hits, [1, 2])

    def test_rule_flags_checker_local_mapped_key_proof_wrappers(self):
        pattern, excludes = self._semantic_proof_check()
        text = "\n".join(
            [
                "fn mapped_key_constraint_filters_current_object_keys(&mut self) -> bool {",
                "fn generic_index_filters_current_type_param_keys(&mut self) -> bool {",
                "let candidates = generic::conditional_key_filter_candidates(db, ty);",
                "let next = generic::instantiate_alias_application_body(db, body, params, args);",
            ]
        )
        hits = self.arch_guard.find_matches(
            text,
            pattern,
            "crates/tsz-checker/src/types/type_checking/indexed_access/mapped_key_check.rs",
            excludes,
        )
        self.assertEqual(hits, [1, 2, 3, 4])

    def test_rule_ignores_query_boundaries_tests_and_comments(self):
        pattern, excludes = self._semantic_proof_check()
        text = "let components = query::full_conditional_type_components(db, ty);"
        query_boundary_hits = self.arch_guard.find_matches(
            text,
            pattern,
            "crates/tsz-checker/src/query_boundaries/checkers/generic.rs",
            excludes,
        )
        test_hits = self.arch_guard.find_matches(
            text,
            pattern,
            "crates/tsz-checker/tests/deferred_conditional_identity_extends_tests.rs",
            excludes,
        )
        comment_hits = self.arch_guard.find_matches(
            "// let components = query::full_conditional_type_components(db, ty);",
            pattern,
            "crates/tsz-checker/src/types/type_checking/indexed_access/mapped_key_check.rs",
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
                "must stay below 3100 LOC (#8226)"
            ):
                return base, limit
        self.fail(
            "checker type-computation size boundary check is missing from "
            "LINE_LIMIT_CHECKS"
        )

    def test_rule_exists_with_expected_limit(self):
        base, limit = self._computation_file_size_check()
        self.assertEqual(limit, 3100)
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
            if (
                name
                == "Core boundary: tsz-core lib facade must stay at current 365 LOC baseline"
            ):
                return path, limit
        self.fail("core lib facade size boundary check is missing from FILE_LINE_LIMIT_CHECKS")

    def test_rule_exists_with_expected_limit(self):
        path, limit = self._core_lib_size_check()
        self.assertEqual(limit, 365)
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
        self.assertEqual(
            limit,
            self.arch_guard.QUERY_BOUNDARY_COMMON_LINE_BASELINE
            + self.arch_guard.QUERY_BOUNDARY_COMMON_LINE_GREEN_HEADROOM,
        )
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

    def test_real_common_file_uses_baseline_or_green_headroom(self):
        path, limit = self._query_common_size_check()
        baseline = self.arch_guard.QUERY_BOUNDARY_COMMON_LINE_BASELINE
        headroom = self.arch_guard.QUERY_BOUNDARY_COMMON_LINE_GREEN_HEADROOM
        live_lines = len(path.read_text(encoding="utf-8").splitlines())
        self.assertEqual(limit, baseline + headroom)
        self.assertGreaterEqual(
            live_lines,
            baseline,
            "query_boundaries/common.rs dropped below the pinned baseline; "
            "ratchet the baseline/headroom down.",
        )
        self.assertLessEqual(
            live_lines,
            limit,
            "query_boundaries/common.rs exhausted the #14351 green headroom.",
        )


class ArchGuardSolverEngineSizeBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def _generic_call_resolver_size_check(self):
        for entry in self.arch_guard.FILE_LINE_LIMIT_CHECKS:
            name, path, limit = entry
            if "generic call resolver" in name and "#8209" in name:
                return path, limit
        self.fail(
            "generic call resolver size boundary check is missing from "
            "FILE_LINE_LIMIT_CHECKS"
        )

    def test_rule_exists_with_current_limit(self):
        path, limit = self._generic_call_resolver_size_check()
        self.assertEqual(limit, 3413)
        self.assertTrue(
            str(path).endswith(
                "crates/tsz-solver/src/operations/generic_call/resolve.rs"
            )
        )

    def test_real_generic_call_resolver_passes_at_pinned_limit(self):
        path, limit = self._generic_call_resolver_size_check()
        hits = self.arch_guard.scan_file_line_limit(path, limit)
        self.assertEqual(
            hits,
            [],
            "generic call resolver cap is too tight for the live file",
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
        """No single crate's LINE_LIMIT_CHECKS allowlist may exceed the ceiling."""
        # Per-entry ceiling: a single crate root may grandfather at most this
        # many already-over-2000 files. When a file drops below 2000 lines,
        # remove it from the allowlist in the same diff; the list only shrinks.
        # 2026-05-31: pruned to the live checker src over-limit set at 11.
        # 2026-08-07: repo-wide coverage landed (#16733) — one entry per
        # crates/*/src root, each carrying its own audited allowlist. The
        # largest at coverage time is tsz-emitter/tsz-solver with 10, so the
        # per-entry ceiling of 11 leaves each crate a shrink-only ratchet.
        MAX_EXCLUDED = 11
        for entry in self.arch_guard.LINE_LIMIT_CHECKS:
            excludes = entry[3] if len(entry) > 3 else set()
            self.assertLessEqual(
                len(excludes),
                MAX_EXCLUDED,
                f"LINE_LIMIT_CHECKS exclusion list for {entry[0]!r} has "
                f"{len(excludes)} entries, max allowed is {MAX_EXCLUDED}. "
                f"Split a file or remove ones that dropped below the limit.",
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

    def test_line_limit_checks_cover_at_least_these_crates(self):
        """#16733: CLAUDE.md states the 2000-LOC cap repo-wide, but only

        `tsz-checker` was ever wired into `LINE_LIMIT_CHECKS`, leaving every
        other crate's drift invisible to `arch-size`. This does not (yet)
        require full repo coverage — that is a crate-by-crate campaign this
        pins as it lands — but it locks in the crates already brought into
        compliance so a future edit cannot silently drop one back out.
        """
        covered_bases = {entry[1] for entry in self.arch_guard.LINE_LIMIT_CHECKS}
        for crate in ("tsz-checker", "tsz-binder", "tsz-cli"):
            expected = ROOT / "crates" / crate / "src"
            self.assertIn(
                expected,
                covered_bases,
                f"crates/{crate}/src dropped out of LINE_LIMIT_CHECKS coverage.",
            )


class ArchGuardLineLimitCoverageTests(unittest.TestCase):
    """Cover the repo-wide 2000-LOC coverage invariant (#16733).

    The historical gap was that only `tsz-checker/src` was registered in
    `LINE_LIMIT_CHECKS`, so a new file over 2000 lines in any other crate
    passed `arch-size` silently. `scan_line_limit_coverage` makes coverage
    itself a checked invariant: every `crates/*/src` root must be registered.
    """

    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def test_every_crate_src_root_is_registered(self):
        """No `crates/*/src` root may be missing from LINE_LIMIT_CHECKS."""
        missing = self.arch_guard.scan_line_limit_coverage()
        self.assertEqual(
            missing,
            [],
            "Unguarded crate src roots (documented 2000-LOC cap not enforced): "
            + ", ".join(missing),
        )

    def test_registered_bases_are_real_crate_src_roots(self):
        """Every 2000-LOC base must be an existing `crates/*/src` directory."""
        roots = {p.resolve() for p in self.arch_guard.crate_src_roots()}
        for name, base, limit, *_rest in self.arch_guard.LINE_LIMIT_CHECKS:
            if limit != self.arch_guard.SRC_LINE_LIMIT:
                continue
            self.assertIn(
                base.resolve(),
                roots,
                f"LINE_LIMIT_CHECKS base for {name!r} is not a crates/*/src root: {base}",
            )

    def test_coverage_flags_a_missing_root(self):
        """A crate whose src root is unregistered is reported as missing."""
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            crates_dir = pathlib.Path(temp_dir) / "crates"
            (crates_dir / "tsz-registered" / "src").mkdir(parents=True)
            (crates_dir / "tsz-unguarded" / "src").mkdir(parents=True)
            checks = [
                (
                    "registered",
                    crates_dir / "tsz-registered" / "src",
                    self.arch_guard.SRC_LINE_LIMIT,
                    set(),
                ),
            ]
            missing = self.arch_guard.scan_line_limit_coverage(
                checks, crates_dir=crates_dir
            )
            self.assertIn("tsz-unguarded", "/".join(missing))
            self.assertNotIn("tsz-registered", "/".join(missing))

    def test_coverage_ignores_non_default_limit_entries(self):
        """A sub-directory entry at a different limit does not register a root."""
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            crates_dir = pathlib.Path(temp_dir) / "crates"
            (crates_dir / "tsz-demo" / "src").mkdir(parents=True)
            checks = [
                (
                    "computation-style sub-limit",
                    crates_dir / "tsz-demo" / "src" / "computation",
                    3100,
                ),
            ]
            missing = self.arch_guard.scan_line_limit_coverage(
                checks, crates_dir=crates_dir
            )
            self.assertEqual(len(missing), 1)
            self.assertTrue(missing[0].endswith("crates/tsz-demo/src"))


class ArchGuardTestsLineLimitTests(unittest.TestCase):
    """Cover `TESTS_LINE_LIMIT_CHECKS` + its coverage invariant (#16745).

    Mirrors `ArchGuardLineLimitCoverageTests` for the src-root cap (#16733):
    the 2000-LOC cap is documented over "source, test, script" without
    qualification, so a `crates/*/tests` root that is never scanned should
    fail the same way an unregistered `crates/*/src` root would.
    """

    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def test_every_crate_tests_root_is_registered(self):
        """No `crates/*/tests` root may be missing from TESTS_LINE_LIMIT_CHECKS."""
        missing = self.arch_guard.scan_tests_line_limit_coverage()
        self.assertEqual(
            missing,
            [],
            "Unguarded crate tests roots (documented 2000-LOC cap not enforced): "
            + ", ".join(missing),
        )

    def test_registered_bases_are_real_crate_tests_roots(self):
        """Every TESTS_LINE_LIMIT_CHECKS base must be an existing crates/*/tests dir."""
        roots = {p.resolve() for p in self.arch_guard.crate_tests_roots()}
        for name, base, _limit, *_rest in self.arch_guard.TESTS_LINE_LIMIT_CHECKS:
            self.assertIn(
                base.resolve(),
                roots,
                f"TESTS_LINE_LIMIT_CHECKS base for {name!r} is not a "
                f"crates/*/tests root: {base}",
            )

    def test_coverage_flags_a_missing_tests_root(self):
        """A crate whose tests root is unregistered is reported as missing."""
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            crates_dir = pathlib.Path(temp_dir) / "crates"
            (crates_dir / "tsz-registered" / "tests").mkdir(parents=True)
            (crates_dir / "tsz-unguarded" / "tests").mkdir(parents=True)
            checks = [
                (
                    "registered",
                    crates_dir / "tsz-registered" / "tests",
                    self.arch_guard.TESTS_LINE_LIMIT,
                    set(),
                ),
            ]
            missing = self.arch_guard.scan_tests_line_limit_coverage(
                checks, crates_dir=crates_dir
            )
            self.assertIn("tsz-unguarded", "/".join(missing))
            self.assertNotIn("tsz-registered", "/".join(missing))

    def test_excluded_tests_files_actually_exist(self):
        """Every file in a TESTS_LINE_LIMIT_CHECKS exclusion list must exist on disk."""
        for entry in self.arch_guard.TESTS_LINE_LIMIT_CHECKS:
            excludes = entry[3] if len(entry) > 3 else set()
            for rel_path in excludes:
                full_path = ROOT / rel_path
                self.assertTrue(
                    full_path.exists(),
                    f"Excluded file {rel_path} does not exist. Remove it from the exclusion list.",
                )

    def test_excluded_tests_files_actually_exceed_limit(self):
        """Every excluded tests file must actually be over the limit."""
        for entry in self.arch_guard.TESTS_LINE_LIMIT_CHECKS:
            limit = entry[2]
            excludes = entry[3] if len(entry) > 3 else set()
            for rel_path in excludes:
                full_path = ROOT / rel_path
                if not full_path.exists():
                    continue  # caught by test_excluded_tests_files_actually_exist
                with full_path.open("r", encoding="utf-8", errors="ignore") as fh:
                    line_count = sum(1 for _ in fh)
                self.assertGreater(
                    line_count,
                    limit,
                    f"Excluded file {rel_path} has {line_count} lines "
                    f"(limit {limit}). Remove it from the exclusion list.",
                )

    def test_tests_line_limit_exclusion_count_cannot_grow(self):
        """No single crate's TESTS_LINE_LIMIT_CHECKS allowlist may exceed the ceiling."""
        # Largest allowlist at landing time (tsz-checker/tsz-cli/tsz-solver) is 2.
        MAX_EXCLUDED = 2
        for entry in self.arch_guard.TESTS_LINE_LIMIT_CHECKS:
            excludes = entry[3] if len(entry) > 3 else set()
            self.assertLessEqual(
                len(excludes),
                MAX_EXCLUDED,
                f"TESTS_LINE_LIMIT_CHECKS exclusion list for {entry[0]!r} has "
                f"{len(excludes)} entries, max allowed is {MAX_EXCLUDED}. "
                f"Split a file or remove ones that dropped below the limit.",
            )


class ArchGuardScriptsLineLimitTests(unittest.TestCase):
    """Cover `SCRIPTS_LINE_LIMIT_CHECKS` + `scan_script_line_limits` (#16745)."""

    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def test_scripts_root_is_registered(self):
        bases = {entry[1] for entry in self.arch_guard.SCRIPTS_LINE_LIMIT_CHECKS}
        self.assertIn(ROOT / "scripts", bases)

    def test_scripts_allowlist_is_currently_empty(self):
        """Splitting arch_guard_shared.py (#16745) leaves no grandfathered debt."""
        for _name, _base, _limit, allowlist in self.arch_guard.SCRIPTS_LINE_LIMIT_CHECKS:
            self.assertEqual(allowlist, set())

    def test_scan_script_line_limits_flags_file_above_limit(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            base = pathlib.Path(temp_dir)
            big = base / "big.py"
            big.write_text("\n".join(f"x = {i}" for i in range(5)) + "\n")
            hits = self.arch_guard.scan_script_line_limits(base, 3)
            self.assertEqual(len(hits), 1)
            self.assertIn("big.py", hits[0])

    def test_scan_script_line_limits_allows_file_at_limit(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            base = pathlib.Path(temp_dir)
            ok = base / "ok.sh"
            ok.write_text("\n".join(f"echo {i}" for i in range(3)) + "\n")
            hits = self.arch_guard.scan_script_line_limits(base, 3)
            self.assertEqual(hits, [])

    def test_scan_script_line_limits_ignores_non_script_extensions(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            base = pathlib.Path(temp_dir)
            data = base / "data.json"
            data.write_text("\n".join(f'"{i}"' for i in range(5)) + "\n")
            hits = self.arch_guard.scan_script_line_limits(base, 3)
            self.assertEqual(hits, [])

    def test_scan_script_line_limits_honors_exclude_files(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            base = pathlib.Path(temp_dir)
            big = base / "big.mjs"
            big.write_text("\n".join(f"x = {i}" for i in range(5)) + "\n")
            rel = big.relative_to(ROOT).as_posix()
            hits = self.arch_guard.scan_script_line_limits(base, 3, exclude_files={rel})
            self.assertEqual(hits, [])


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




class ArchGuardAllFileLimitChecksPassTests(unittest.TestCase):
    """Generic guard: every FILE_LINE_LIMIT_CHECKS entry must exist on disk
    and must not exceed its pinned cap.  This catches entries (like
    async_es5_ir.rs) that have no dedicated per-entry test class."""

    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def test_all_file_limit_paths_exist(self):
        for name, path, _limit in self.arch_guard.FILE_LINE_LIMIT_CHECKS:
            self.assertTrue(
                path.exists(),
                f"FILE_LINE_LIMIT_CHECKS entry '{name}': path {path} does not exist. "
                "Remove the entry or fix the path.",
            )

    def test_all_file_limit_caps_are_not_exceeded(self):
        for name, path, limit in self.arch_guard.FILE_LINE_LIMIT_CHECKS:
            hits = self.arch_guard.scan_file_line_limit(path, limit)
            self.assertEqual(
                hits,
                [],
                f"FILE_LINE_LIMIT_CHECKS entry '{name}': cap {limit} is too tight "
                f"for the live file ({hits}). Bump the cap or split the file.",
            )

    def test_no_unguarded_oversized_production_files(self):
        """Every production .rs file over 2000 lines must appear in FILE_LINE_LIMIT_CHECKS.

        Prevents new monoliths from growing unchecked. When a file legitimately exceeds
        2000 lines, add a FILE_LINE_LIMIT_CHECKS entry for it in the same PR; ratchet
        the cap down as the file is split per §19.
        """
        arch_guard = self.arch_guard
        guarded = {
            pathlib.Path(path).resolve()
            for _, path, _ in arch_guard.FILE_LINE_LIMIT_CHECKS
        }
        limit = 2000
        unguarded = []
        crates_root = ROOT / "crates"
        if not crates_root.exists():
            return
        for path in crates_root.rglob("*.rs"):
            rel = path.relative_to(ROOT).as_posix()
            rel_parts = set(rel.split("/"))
            if arch_guard.EXCLUDE_DIRS.intersection(rel_parts):
                continue
            if "tests" in rel_parts or "benches" in rel_parts:
                continue
            if arch_guard.is_test_file(rel):
                continue
            if path.resolve() in guarded:
                continue
            try:
                n = len(path.read_text(encoding="utf-8", errors="ignore").splitlines())
            except OSError:
                continue
            if n > limit:
                unguarded.append((n, rel))
        if unguarded:
            unguarded.sort(reverse=True)
            lines = "\n".join(f"  {n:5d}  {r}" for n, r in unguarded)
            self.fail(
                f"Found {len(unguarded)} production file(s) over {limit} lines with "
                f"no FILE_LINE_LIMIT_CHECKS guard.\n"
                f"Add an entry for each file in the same PR that grows it past {limit} "
                f"lines; ratchet down as submodules land (§19):\n{lines}"
            )


class ArchGuardAllowlistRatchetCoverageTests(unittest.TestCase):
    """Cover the "allowlisted implies ratcheted" invariant (#16488).

    #16733/#16745 registered every `crates/*/src` and `crates/*/tests` root
    under the per-crate 2000-LOC cap, but a file named in one of those crates'
    allowlists is *exempt* from the directory cap. Without a matching
    `FILE_LINE_LIMIT_CHECKS` ratchet such a file is bounded by nothing and can
    grow arbitrarily while `arch-size` stays green — the residual hole #16488
    reports. `scan_allowlist_ratchet_coverage` makes the pairing a checked
    invariant so the ceiling cannot silently reopen for a newly allowlisted
    file.
    """

    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def test_every_allowlisted_file_is_ratcheted(self):
        """No allowlisted file may lack a per-file size ratchet on main."""
        gap = self.arch_guard.scan_allowlist_ratchet_coverage()
        self.assertEqual(
            gap,
            [],
            "Allowlisted files exempt from the 2000-LOC cap but with no "
            "FILE_LINE_LIMIT_CHECKS ratchet (can grow unbounded): "
            + ", ".join(gap),
        )

    def test_flags_allowlisted_file_without_ratchet(self):
        """An allowlisted file with no ratchet entry is reported."""
        gap = self.arch_guard.scan_allowlist_ratchet_coverage(
            src_allowlists=[("tsz-demo", "Demo", {"crates/tsz-demo/src/huge.rs"})],
            tests_allowlists=[],
            file_line_checks=[],
        )
        self.assertEqual(gap, ["crates/tsz-demo/src/huge.rs"])

    def test_ratcheted_allowlisted_file_is_not_flagged(self):
        """An allowlisted file that carries a ratchet is not reported."""
        rel = "crates/tsz-demo/src/huge.rs"
        gap = self.arch_guard.scan_allowlist_ratchet_coverage(
            src_allowlists=[("tsz-demo", "Demo", {rel})],
            tests_allowlists=[],
            file_line_checks=[("Demo ratchet", ROOT / rel, 2500)],
        )
        self.assertEqual(gap, [])

    def test_tests_allowlist_entries_are_covered_too(self):
        """The tests-tree allowlist is scanned alongside the src allowlist."""
        rel = "crates/tsz-demo/tests/huge_tests.rs"
        gap = self.arch_guard.scan_allowlist_ratchet_coverage(
            src_allowlists=[],
            tests_allowlists=[("tsz-demo", "Demo", {rel})],
            file_line_checks=[],
        )
        self.assertEqual(gap, [rel])


class ArchGuardCeilingContractLimitTests(unittest.TestCase):
    """Cover the "no ceiling may legalize crossing 2000 lines" invariant (#17295).

    `.claude/CLAUDE.md` caps hand-authored files at 2000 physical lines, but
    `FILE_LINE_LIMIT_CHECKS` enforces per-file ceilings instead, and most sit
    above 2000 — a local ceiling legalizing exactly the growth the contract
    forbids. `scan_ceiling_contract_violations` freezes every over-limit
    ceiling at its recorded `LEGACY_CEILING_DEBT` value: a new ceiling above
    2000 is always a violation, and an existing one may only be lowered.
    """

    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def test_main_has_no_ceiling_contract_violations(self):
        """Every live FILE_LINE_LIMIT_CHECKS ceiling is <=2000 or frozen debt."""
        violations = self.arch_guard.scan_ceiling_contract_violations()
        self.assertEqual(
            violations,
            [],
            "FILE_LINE_LIMIT_CHECKS ceilings that cross the 2000-line "
            "contract limit without a matching frozen LEGACY_CEILING_DEBT "
            "entry: " + ", ".join(violations),
        )

    def test_ceiling_at_or_under_limit_is_never_flagged(self):
        """A ceiling <=2000 needs no legacy-debt entry at all."""
        violations = self.arch_guard.scan_ceiling_contract_violations(
            checks=[("Demo ratchet", ROOT / "crates/tsz-demo/src/lib.rs", 2000)],
            legacy_debt={},
        )
        self.assertEqual(violations, [])

    def test_new_ceiling_above_limit_with_no_debt_entry_is_flagged(self):
        """A fresh ceiling above 2000 is rejected outright."""
        violations = self.arch_guard.scan_ceiling_contract_violations(
            checks=[("Demo ratchet", ROOT / "crates/tsz-demo/src/lib.rs", 2001)],
            legacy_debt={},
        )
        self.assertEqual(len(violations), 1)
        self.assertIn("crates/tsz-demo/src/lib.rs", violations[0])
        self.assertIn("no LEGACY_CEILING_DEBT entry", violations[0])

    def test_legacy_ceiling_at_frozen_value_passes(self):
        """A legacy over-limit ceiling matching its frozen value is fine."""
        rel = "crates/tsz-demo/src/lib.rs"
        violations = self.arch_guard.scan_ceiling_contract_violations(
            checks=[("Demo ratchet", ROOT / rel, 2500)],
            legacy_debt={rel: 2500},
        )
        self.assertEqual(violations, [])

    def test_legacy_ceiling_lowered_below_frozen_value_passes(self):
        """Paying down a legacy ceiling (still above 2000) stays clean."""
        rel = "crates/tsz-demo/src/lib.rs"
        violations = self.arch_guard.scan_ceiling_contract_violations(
            checks=[("Demo ratchet", ROOT / rel, 2400)],
            legacy_debt={rel: 2500},
        )
        self.assertEqual(violations, [])

    def test_legacy_ceiling_raised_above_frozen_value_is_flagged(self):
        """Raising an existing over-limit ceiling is the exact regression #17295 reports."""
        rel = "crates/tsz-demo/src/lib.rs"
        violations = self.arch_guard.scan_ceiling_contract_violations(
            checks=[("Demo ratchet", ROOT / rel, 2600)],
            legacy_debt={rel: 2500},
        )
        self.assertEqual(len(violations), 1)
        self.assertIn(rel, violations[0])
        self.assertIn("may only be lowered, never raised", violations[0])

    def test_legacy_ceiling_paid_down_to_or_under_limit_needs_no_ledger_edit(self):
        """Once a legacy ceiling drops to <=2000 the stale debt entry is inert."""
        rel = "crates/tsz-demo/src/lib.rs"
        violations = self.arch_guard.scan_ceiling_contract_violations(
            checks=[("Demo ratchet", ROOT / rel, 2000)],
            legacy_debt={rel: 2500},
        )
        self.assertEqual(violations, [])


if __name__ == "__main__":
    unittest.main()
