#!/usr/bin/env python3
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("cache-visibility-report.py")


def load_module():
    spec = importlib.util.spec_from_file_location("cache_visibility_report", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class CacheVisibilityReportTests(unittest.TestCase):
    def setUp(self):
        self.module = load_module()

    def write(self, root: Path, rel: str, text: str) -> None:
        path = root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def test_scan_reports_cache_fields_and_ignores_tests(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            self.write(
                root,
                "src/lib.rs",
                "\n".join(
                    [
                        "pub struct QueryCache {",
                        "    eval_cache: RefCell<FxHashMap<EvalCacheKey, TypeId>>,",
                        "    eval_cache_hits: u64,",
                        "    eval_cache_misses: u64,",
                        "    eval_cache_entries: usize,",
                        "    scratch: FxHashMap<TypeId, TypeId>,",
                        "}",
                        "impl QueryCache {",
                        "    pub fn estimated_size_bytes(&self) -> usize { self.eval_cache_entries }",
                        "}",
                        "pub struct Local {",
                        "    local_cache: rustc_hash::FxHashMap<u32, bool>,",
                        "}",
                    ]
                )
                + "\n",
            )
            self.write(
                root,
                "src/tests/ignored.rs",
                "pub struct Test { test_cache: FxHashMap<u32, bool> }\n",
            )
            candidates = self.module.scan([root / "src"])

        self.assertEqual([candidate.name for candidate in candidates], ["eval_cache", "local_cache"])
        covered, review = candidates
        self.assertFalse(covered.needs_review)
        self.assertTrue(review.needs_review)
        self.assertEqual(covered.retention, "retained")
        self.assertEqual(review.retention, "unknown")

    def test_type_aliases_are_reported(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            self.write(root, "src/lib.rs", "pub type ScopeCache = FxHashMap<u32, Vec<Symbol>>;\n")
            candidates = self.module.scan([root / "src"])

        self.assertEqual(len(candidates), 1)
        self.assertEqual(candidates[0].owner, "<module>")
        self.assertEqual(candidates[0].name, "ScopeCache")
        self.assertEqual(candidates[0].type, "FxHashMap<u32, Vec<Symbol>>")
        self.assertEqual(candidates[0].retention, "module")

    def test_scan_ignores_cache_shapes_in_block_comments_and_strings(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            self.write(
                root,
                "src/lib.rs",
                "\n".join(
                    [
                        "/*",
                        "pub struct Commented {",
                        "    ghost_cache: FxHashMap<u32, bool>,",
                        "}",
                        "*/",
                        "pub struct Real {",
                        "    rendered: &'static str,",
                        "    real_cache: FxHashMap<u32, bool>,",
                        "}",
                        'const SAMPLE: &str = "sample_cache: FxHashMap<u32, bool>";',
                    ]
                )
                + "\n",
            )
            candidates = self.module.scan([root / "src"])

        self.assertEqual([candidate.name for candidate in candidates], ["real_cache"])

    def test_exact_cache_field_names_are_reported(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            self.write(
                root,
                "src/lib.rs",
                "\n".join(
                    [
                        "pub struct LibLoader {",
                        "    cache: FxHashMap<String, String>,",
                        "    not_cache: Vec<String>,",
                        "}",
                        "impl LibLoader {",
                        "    pub fn cache_size(&self) -> usize { self.cache.len() }",
                        "}",
                    ]
                )
                + "\n",
            )
            candidates = self.module.scan([root / "src"])

        self.assertEqual([candidate.name for candidate in candidates], ["cache"])
        self.assertEqual(candidates[0].owner, "LibLoader")
        self.assertFalse(candidates[0].needs_review)

    def test_retention_classifies_known_operation_local_and_snapshot_caches(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            self.write(
                root,
                "src/lib.rs",
                "\n".join(
                    [
                        "pub struct TypeEvaluator {",
                        "    conditional_subtype_cache: FxHashMap<(TypeId, TypeId), bool>,",
                        "}",
                        "pub struct CacheSnapshot {",
                        "    flow_analysis_cache: rustc_hash::FxHashMap<(u32, u32), bool>,",
                        "}",
                    ]
                )
                + "\n",
            )
            candidates = self.module.scan([root / "src"])

        by_name = {candidate.name: candidate for candidate in candidates}
        self.assertEqual(by_name["conditional_subtype_cache"].retention, "operation_local")
        self.assertEqual(by_name["flow_analysis_cache"].retention, "snapshot")

    def test_retained_path_only_applies_to_module_aliases(self):
        self.assertEqual(
            self.module.classify_retention(
                "crates/tsz-checker/src/flow/control_flow/core.rs",
                "<module>",
            ),
            "retained",
        )
        self.assertEqual(
            self.module.classify_retention(
                "crates/tsz-checker/src/flow/control_flow/core.rs",
                "FlowAnalyzer",
            ),
            "operation_local",
        )

    def test_retained_only_filters_json_output(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            self.write(
                root,
                "src/lib.rs",
                "\n".join(
                    [
                        "pub struct CheckerContext {",
                        "    lib_type_resolution_cache: FxHashMap<String, bool>,",
                        "}",
                        "pub struct TypeEvaluator {",
                        "    conditional_subtype_cache: FxHashMap<(TypeId, TypeId), bool>,",
                        "}",
                    ]
                )
                + "\n",
            )
            result = subprocess.run(
                [
                    "python3",
                    str(SCRIPT),
                    "--root",
                    str(root / "src"),
                    "--retained-only",
                    "--json",
                ],
                check=False,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        self.assertEqual(
            [candidate["name"] for candidate in payload["candidates"]],
            ["lib_type_resolution_cache"],
        )
        self.assertEqual(payload["summary"]["candidates_by_retention"], {"retained": 1})

    def test_exact_cache_type_alias_is_reported(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            self.write(root, "src/lib.rs", "type Cache = FxHashMap<u32, bool>;\n")
            candidates = self.module.scan([root / "src"])

        self.assertEqual(len(candidates), 1)
        self.assertEqual(candidates[0].owner, "<module>")
        self.assertEqual(candidates[0].name, "Cache")

    def test_multiline_cache_field_type_is_reported(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            self.write(
                root,
                "src/lib.rs",
                "\n".join(
                    [
                        "pub struct ModuleResolver {",
                        "    skip_fallback_cache:",
                        "        std::cell::RefCell<FxHashMap<SkipFallbackCacheKey, bool>>,",
                        "}",
                    ]
                )
                + "\n",
            )
            candidates = self.module.scan([root / "src"])

        self.assertEqual([candidate.name for candidate in candidates], ["skip_fallback_cache"])
        self.assertEqual(
            candidates[0].type,
            "std::cell::RefCell<FxHashMap<SkipFallbackCacheKey, bool>>",
        )

    def test_multiline_cache_type_alias_is_reported(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            self.write(
                root,
                "src/lib.rs",
                "\n".join(
                    [
                        "type NestedCache = RefCell<",
                        "    FxHashMap<String, Vec<SymbolId>>,",
                        ">;",
                    ]
                )
                + "\n",
            )
            candidates = self.module.scan([root / "src"])

        self.assertEqual([candidate.name for candidate in candidates], ["NestedCache"])
        self.assertEqual(candidates[0].owner, "<module>")

    def test_camel_case_alias_matches_snake_case_statistics(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            self.write(
                root,
                "src/lib.rs",
                "\n".join(
                    [
                        "type LibFileCache = FxHashMap<u32, bool>;",
                        "pub struct LibFileCacheStatistics {",
                        "    lib_file_cache_entries: usize,",
                        "    lib_file_cache_hits: u64,",
                        "    lib_file_cache_misses: u64,",
                        "}",
                        "impl LibFileCacheStatistics {",
                        "    fn estimated_size_bytes(&self) -> usize {",
                        "        self.lib_file_cache_entries",
                        "    }",
                        "}",
                    ]
                )
                + "\n",
            )
            candidates = self.module.scan([root / "src"])

        self.assertEqual(len(candidates), 1)
        self.assertFalse(candidates[0].needs_review)

    def test_generic_statistics_method_does_not_cover_unmentioned_cache(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            self.write(
                root,
                "src/lib.rs",
                "\n".join(
                    [
                        "pub struct QueryCache {",
                        "    eval_cache: RefCell<FxHashMap<EvalCacheKey, TypeId>>,",
                        "    intersection_merge_cache: RefCell<FxHashMap<TypeId, Option<TypeId>>>,",
                        "}",
                        "pub struct QueryCacheStatistics {",
                        "    eval_cache_entries: usize,",
                        "}",
                        "impl QueryCacheStatistics {",
                        "    pub fn estimated_size_bytes(&self) -> usize { self.eval_cache_entries }",
                        "}",
                    ]
                )
                + "\n",
            )

            candidates = self.module.scan([root / "src"])

        by_name = {candidate.name: candidate for candidate in candidates}
        self.assertFalse(by_name["eval_cache"].needs_review)
        self.assertTrue(by_name["intersection_merge_cache"].needs_review)

    def test_statistics_in_separate_module_cover_cache_field(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            self.write(
                root,
                "src/context/mod.rs",
                "\n".join(
                    [
                        "pub struct CheckerContext {",
                        "    lib_type_resolution_cache: FxHashMap<String, bool>,",
                        "}",
                    ]
                )
                + "\n",
            )
            self.write(
                root,
                "src/context/cache_statistics.rs",
                "\n".join(
                    [
                        "pub struct CheckerContextCacheStatistics {",
                        "    lib_type_resolution_cache_entries: usize,",
                        "    lib_type_resolution_cache_estimated_size_bytes: usize,",
                        "}",
                    ]
                )
                + "\n",
            )

            candidates = self.module.scan([root / "src"])

        self.assertEqual(len(candidates), 1)
        self.assertFalse(candidates[0].needs_review)

    def test_solver_visitor_predicate_memos_are_visible(self):
        root = Path(__file__).resolve().parents[2]
        candidates = self.module.scan([root / "crates/tsz-solver/src/visitors"])
        predicate_memos = {
            candidate.owner: candidate
            for candidate in candidates
            if candidate.path == "crates/tsz-solver/src/visitors/visitor_predicates.rs"
            and candidate.name == "memo"
        }

        self.assertEqual(
            set(predicate_memos),
            {
                "ContainsTypeChecker",
                "FreeTypeParamChecker",
                "FreeInferChecker",
                "ShallowContainsTypeChecker",
            },
        )
        self.assertFalse(
            any(candidate.needs_review for candidate in predicate_memos.values()),
            predicate_memos,
        )

    def test_default_roots_include_binder_cache_surfaces(self):
        self.assertIn("crates/tsz-binder/src", self.module.DEFAULT_ROOTS)

    def test_cli_json_output_is_machine_readable(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            self.write(
                root,
                "src/lib.rs",
                "\n".join(
                    [
                        "pub struct Resolver {",
                        "    resolution_cache: FxHashMap<String, bool>,",
                        "}",
                    ]
                )
                + "\n",
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
        self.assertEqual(payload["summary"]["schema_version"], 2)
        self.assertEqual(payload["summary"]["total_candidates"], 1)
        self.assertEqual(payload["summary"]["needs_review"], 1)


if __name__ == "__main__":
    unittest.main()
