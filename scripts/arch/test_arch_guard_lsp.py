from test_arch_guard_support import load_arch_guard_module, pathlib, tempfile, unittest


class ArchGuardLspFeatureMethodCountTests(unittest.TestCase):
    """Cover `LSP_FEATURE_METHOD_COUNT_CHECKS` + `scan_lsp_feature_method_count`.

    Architecture health metric 7 anchor - workstream 6 ("LSP And WASM
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
        # find_, resolve_) - cap=0 should fire all 7 plus a summary.
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
            "/// pub fn get_hover() {} - example in doc comment\n"
            "pub fn get_top_level() {}\n"
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
        # Cap exactly equal to live count -> no hits.
        self.assertEqual(self.arch_guard.scan_lsp_feature_method_count(path, 2), [])
        # Cap above live count -> still no hits.
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
                f"{name}: cap is too tight - guard fires at the live count.",
            )


class ArchGuardSpeculationGuardNameTests(unittest.TestCase):
    """Pin architecture health metric 6.

    Metric 6 tracks speculation APIs with surprising non-RAII behavior and
    verifies `scan_speculation_guard_struct_count`.
    """

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
        `...Guard` structs (PR #1213 already renamed the only offender)."""
        for entry in self.arch_guard.SPECULATION_GUARD_NAME_CHECKS:
            name, file_path, max_guard_count = entry
            hits = self.arch_guard.scan_speculation_guard_struct_count(
                file_path, max_guard_count
            )
            self.assertEqual(
                hits,
                [],
                f"{name}: cap is too tight - guard fires at the live count.",
            )


if __name__ == "__main__":
    unittest.main()
