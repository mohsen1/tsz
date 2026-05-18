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

    def test_type_aliases_are_reported(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            self.write(root, "src/lib.rs", "pub type ScopeCache = FxHashMap<u32, Vec<Symbol>>;\n")
            candidates = self.module.scan([root / "src"])

        self.assertEqual(len(candidates), 1)
        self.assertEqual(candidates[0].owner, "<module>")
        self.assertEqual(candidates[0].name, "ScopeCache")

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
        self.assertEqual(payload["summary"]["schema_version"], 1)
        self.assertEqual(payload["summary"]["total_candidates"], 1)
        self.assertEqual(payload["summary"]["needs_review"], 1)


if __name__ == "__main__":
    unittest.main()
