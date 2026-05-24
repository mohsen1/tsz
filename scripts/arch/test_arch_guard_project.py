import pathlib
import tempfile
import unittest

from arch_guard_test_support import ROOT, load_arch_guard_module


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
export const COMPILE_CANARY_PROJECT_ROWS = ["type-challenges-solutions-project"];
"""
        fixtures = """
tsz_project_fixture_sources() {
  case "$1" in
    utility-types-project)
      printf 'utility-types|repo|ref\\n'
      ;;
    type-challenges-solutions-project)
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
    name: "type-challenges-solutions-project",
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
    type-challenges-solutions-project)
      printf 'type-challenges-solutions|repo|ref\\n'
      ;;
  esac
}
"""
        self.assertEqual(self._write_and_scan(rows, fixtures), [])

    def test_generated_project_source_cases_are_allowed_without_static_pins(self):
        rows = """
export const REQUIRED_PROJECT_ROWS = ["utility-types-project", "vite-vanilla-ts-app"];
export const COMPILE_CANARY_PROJECT_ROWS = [];
"""
        fixtures = """
tsz_project_fixture_sources() {
  case "$1" in
    utility-types-project)
      printf 'utility-types|repo|ref\\n'
      ;;
    vite-vanilla-ts-app)
      ;;
  esac
}
"""
        self.assertEqual(self._write_and_scan(rows, fixtures), [])

    def test_missing_source_metadata_is_reported(self):
        rows = """
export const REQUIRED_PROJECT_ROWS = ["utility-types-project"];
export const COMPILE_CANARY_PROJECT_ROWS = ["type-challenges-solutions-project"];
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
        self.assertIn("missing fixture source metadata for type-challenges-solutions-project", hits[0])

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
export const COMPILE_CANARY_PROJECT_ROWS = ["type-challenges-solutions-project"];
"""
        fixtures = """
tsz_project_fixture_sources() {
  case "$1" in
    utility-types-project)
      printf 'utility-types|repo\\n'
      ;;
    type-challenges-solutions-project)
      printf 'type-challenges-solutions|repo|\\n'
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
            any("malformed fixture source metadata for type-challenges-solutions-project" in hit for hit in hits),
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
export const COMPILE_CANARY_PROJECT_ROWS = ["type-challenges-solutions-project"];
"""
        compile_guard = """
if should_check_project "utility-types-project"; then :; fi
if should_check_project "vite-vanilla-ts-app"; then :; fi
if should_check_project "type-challenges-solutions-project"; then :; fi
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
export const COMPILE_CANARY_PROJECT_ROWS = ["type-challenges-solutions-project"];
"""
        compile_guard = 'if should_check_project "type-challenges-solutions-project"; then :; fi'
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

    def test_flags_rendered_message_predicates(self):
        pattern, _max_lines = self._check_by_name("rendered message predicates")
        root = self._make_tree(
            {
                "crates/tsz-checker/src/checkers/jsx/rendered.rs": (
                    'if display.starts_with("IntrinsicAttributes") {}\n'
                    'if target_display.ends_with(", Element>") {}\n'
                    'if diagnostic.message_text.contains("Type") {}\n'
                    'if source_text.contains("fixture shape") {}\n'
                ),
            }
        )
        hits = self.arch_guard.scan_regex_line_count([root], pattern, 0)
        self.assertEqual(len(hits), 4, f"unexpected hits: {hits!r}")
        self.assertIn("rendered.rs:1", hits[0])
        self.assertIn("rendered.rs:2", hits[1])
        self.assertIn("rendered.rs:3", hits[2])

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
        pattern, _max_lines = self._check_by_name("legacy packed relation flag bridges")
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
        pattern, _max_lines = self._check_by_name("legacy packed relation flag bridges")
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

    def test_relation_engine_packed_apply_flags_guard(self):
        pattern, _max_lines = self._check_by_name("packed apply_flags")
        root = self._make_tree(
            {
                "crates/tsz-solver/src/relations/compat.rs": (
                    "pub fn apply_flags(&mut self, flags: u16) {}\n"
                    "checker.apply_flags(policy.flags);\n"
                    "fn apply_policy(policy: RelationPolicy) {}\n"
                ),
                "crates/tsz-solver/src/relations/subtype/core.rs": (
                    "pub(crate) const fn apply_flags(mut self, flags: u16) -> Self { self }\n"
                ),
            }
        )
        hits = self.arch_guard.scan_regex_line_count([root], pattern, 0)
        self.assertEqual(len(hits), 4, f"unexpected hits: {hits!r}")
        self.assertIn("compat.rs:1", hits[0])
        self.assertIn("compat.rs:2", hits[1])
        self.assertIn("core.rs:1", hits[2])
        self.assertIn("total matching lines: 3", hits[3])

    def test_query_cache_relation_facade_guard(self):
        pattern, _max_lines = self._check_by_name("query cache uses relation facade")
        root = self._make_tree(
            {
                "crates/tsz-solver/src/caches/query_cache.rs": (
                    "let mut checker = configured_compat_checker(db, resolver, policy, context);\n"
                    "let mut checker = configured_subtype_checker(db, resolver, policy, context);\n"
                    "let result = query_relation(db, source, target, kind, policy, context);\n"
                ),
            }
        )
        hits = self.arch_guard.scan_regex_line_count([root], pattern, 0)
        self.assertEqual(len(hits), 3, f"unexpected hits: {hits!r}")
        self.assertIn("query_cache.rs:1", hits[0])
        self.assertIn("query_cache.rs:2", hits[1])
        self.assertIn("total matching lines: 2", hits[2])

    def test_query_cache_trace_labels_use_typed_policy_names(self):
        pattern, _max_lines = self._check_by_name("typed policy names")
        root = self._make_tree(
            {
                "crates/tsz-solver/src/caches/query_cache.rs": (
                    'query_trace::relation_start(id, "is_subtype_of_with_flags", a, b, flags);\n'
                    'query_trace::relation_end(id, "is_assignable_to_with_flags", true, false);\n'
                    'const OP: &str = "is_subtype_of_with_policy";\n'
                ),
            }
        )
        hits = self.arch_guard.scan_regex_line_count([root], pattern, 0)
        self.assertEqual(len(hits), 3, f"unexpected hits: {hits!r}")
        self.assertIn("query_cache.rs:1", hits[0])
        self.assertIn("query_cache.rs:2", hits[1])
        self.assertIn("total matching lines: 2", hits[2])

    def test_query_cache_legacy_flag_override_guard(self):
        pattern, _max_lines = self._check_by_name("legacy flag overrides")
        root = self._make_tree(
            {
                "crates/tsz-solver/src/caches/query_cache.rs": (
                    "fn is_subtype_of_with_flags(&self, source: TypeId, target: TypeId, flags: u16) -> bool { true }\n"
                    "fn is_assignable_to_with_flags(&self, source: TypeId, target: TypeId, flags: u16) -> bool { true }\n"
                    "fn is_subtype_of_with_policy(&self, source: TypeId, target: TypeId, policy: RelationPolicy) -> bool { true }\n"
                ),
            }
        )
        hits = self.arch_guard.scan_regex_line_count([root], pattern, 0)
        self.assertEqual(len(hits), 3, f"unexpected hits: {hits!r}")
        self.assertIn("query_cache.rs:1", hits[0])
        self.assertIn("query_cache.rs:2", hits[1])
        self.assertIn("total matching lines: 2", hits[2])

    def test_query_database_legacy_flag_method_cap(self):
        pattern, max_lines = self._check_by_name("query database legacy flag methods")
        root = self._make_tree(
            {
                "crates/tsz-solver/src/caches/db.rs": (
                    "fn is_subtype_of_with_flags(&self, source: TypeId, target: TypeId, flags: u16) -> bool { true }\n"
                    "fn is_assignable_to_with_flags(&self, source: TypeId, target: TypeId, flags: u16) -> bool { true }\n"
                ),
                "crates/tsz-solver/src/caches/query_cache.rs": (
                    "fn is_assignable_to_with_flags(&self, source: TypeId, target: TypeId, flags: u16) -> bool { true }\n"
                ),
            }
        )
        hits = self.arch_guard.scan_regex_line_count([root], pattern, max_lines)
        self.assertEqual(len(hits), 4, f"unexpected hits: {hits!r}")
        self.assertIn("db.rs:1", hits[0])
        self.assertIn("db.rs:2", hits[1])
        self.assertIn("query_cache.rs:1", hits[2])
        self.assertIn("total matching lines: 3", hits[3])
        self.assertEqual(max_lines, 0)

    def test_flags_checker_migration_with_parent_cache_callsite(self):
        pattern, _max_lines = self._check_by_name("with_parent_cache_attributed")
        root = self._make_tree(
            {
                "crates/tsz-checker/src/migration.rs": (
                    "let checker = CheckerState::with_parent_cache_attributed(parent, reason);\n"
                    "pub fn with_parent_cache_attributed(parent: Parent) -> Self { todo!() }\n"
                    "// CheckerState::with_parent_cache_attributed(parent, reason);\n"
                ),
            }
        )
        hits = self.arch_guard.scan_regex_line_count([root], pattern, 0)
        self.assertEqual(len(hits), 2, f"unexpected hits: {hits!r}")
        self.assertIn("migration.rs:1", hits[0])
        self.assertIn("total matching lines: 1", hits[1])

    def test_flags_checker_migration_overlay_copy_callsite(self):
        pattern, _max_lines = self._check_by_name(
            "copy_symbol_file_targets_to_attributed"
        )
        root = self._make_tree(
            {
                "crates/tsz-checker/src/migration.rs": (
                    "self.ctx.copy_symbol_file_targets_to_attributed(&mut child, reason);\n"
                    "pub fn copy_symbol_file_targets_to_attributed(&self) {}\n"
                    "// self.ctx.copy_symbol_file_targets_to_attributed(&mut child, reason);\n"
                ),
            }
        )
        hits = self.arch_guard.scan_regex_line_count([root], pattern, 0)
        self.assertEqual(len(hits), 2, f"unexpected hits: {hits!r}")
        self.assertIn("migration.rs:1", hits[0])
        self.assertIn("total matching lines: 1", hits[1])

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

    def test_flags_legacy_relation_flag_bridge_surface(self):
        pattern, _max_lines = self._check_by_name("legacy relation flag bridge surface")
        root = self._make_tree(
            {
                "crates/tsz-solver/src/types.rs": (
                    "RelationCacheConfig::from_checker_flags_u16(flags);\n"
                    "CachedAnyMode::from_legacy_u8(raw);\n"
                    "mode.to_legacy_u8();\n"
                    "// CachedAnyMode::from_legacy_u8(commented);\n"
                ),
                "crates/tsz-solver/src/caches/query_cache.rs": (
                    "subtype_cache_config_from_legacy_flags(flags);\n"
                    "assignability_cache_config_from_legacy_flags(flags);\n"
                ),
            }
        )
        hits = self.arch_guard.scan_regex_line_count([root], pattern, 0)
        self.assertEqual(len(hits), 6, f"unexpected hits: {hits!r}")
        self.assertIn("query_cache.rs:1", hits[0])
        self.assertIn("query_cache.rs:2", hits[1])
        self.assertIn("types.rs:1", hits[2])
        self.assertIn("types.rs:2", hits[3])
        self.assertIn("types.rs:3", hits[4])

    def test_flags_relation_policy_packed_flag_storage(self):
        pattern, _max_lines = self._check_by_name("RelationPolicy must store typed flags")
        root = self._make_tree(
            {
                "crates/tsz-solver/src/relations/relation_queries.rs": (
                    "pub struct RelationPolicy {\n"
                    "    flags: u16,\n"
                    "}\n"
                ),
            }
        )
        hits = self.arch_guard.scan_regex_line_count([root], pattern, 0)
        self.assertEqual(len(hits), 2, f"unexpected hits: {hits!r}")
        self.assertIn("relation_queries.rs:2", hits[0])
        self.assertIn("total matching lines: 1", hits[1])

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
        self.assertTrue(any("rendered message predicates" in name for name in names))
        self.assertTrue(any("#8227" in name for name in names))
        self.assertTrue(any("diagnostic-local RelationRequest" in name for name in names))
        self.assertTrue(any("#8207" in name for name in names))
        self.assertTrue(any("#8204" in name for name in names))
        self.assertTrue(any("with_parent_cache_attributed" in name for name in names))
        self.assertTrue(
            any("copy_symbol_file_targets_to_attributed" in name for name in names)
        )

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




if __name__ == "__main__":
    unittest.main()
