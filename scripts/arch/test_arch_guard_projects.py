from test_arch_guard_support import load_arch_guard_module, pathlib, tempfile, unittest


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
