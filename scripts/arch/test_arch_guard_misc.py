import pathlib
import tempfile
import unittest

from arch_guard_test_support import ROOT, load_arch_guard_module


class ArchGuardDebugPrintMacroTests(unittest.TestCase):
    """Cover Track 10's hard debug-print macro guard."""

    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def test_scans_compiler_internal_debug_print_macros(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            source = root / "crates" / "tsz-checker" / "src" / "lib.rs"
            source.parent.mkdir(parents=True)
            source.write_text(
                "\n".join(
                    [
                        "pub fn probe() {",
                        "    let literal = \"println!(not code)\";",
                        "    println!(\"debug\");",
                        "    // eprintln!(\"comment\");",
                        "    dbg!(literal);",
                        "}",
                    ]
                ),
                encoding="utf-8",
            )

            hits = self.arch_guard.scan_debug_print_macros(
                root,
                ("crates/tsz-checker/src",),
            )

        self.assertEqual(
            hits,
            [
                "crates/tsz-checker/src/lib.rs:3 println!: println!(\"debug\");",
                "crates/tsz-checker/src/lib.rs:5 dbg!: dbg!(literal);",
            ],
        )

    def test_real_compiler_internals_have_no_debug_print_macros(self):
        for name, root, scan_dirs in self.arch_guard.DEBUG_PRINT_MACRO_CHECKS:
            hits = self.arch_guard.scan_debug_print_macros(root, scan_dirs)
            self.assertEqual(hits, [], f"{name}: {hits[:5]}")

    def test_debug_print_check_is_registered(self):
        names = [entry[0] for entry in self.arch_guard.DEBUG_PRINT_MACRO_CHECKS]
        self.assertTrue(
            any("debug print macros" in name for name in names),
            "debug print macro guard missing from DEBUG_PRINT_MACRO_CHECKS",
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

    def test_registered_check_covers_solver_sources(self):
        _name, search_roots, allowlist = self._registered_check()
        root_strings = {str(root) for root in search_roots}

        self.assertIn(str(self.arch_guard.ROOT / "crates" / "tsz-solver" / "src"), root_strings)
        self.assertIn(
            (
                "crates/tsz-solver/src/evaluation/evaluate_rules/infer_pattern.rs",
                "let mut alias_visited = visited.clone();",
            ),
            allowlist,
        )
        self.assertIn(
            (
                "crates/tsz-solver/src/evaluation/evaluate_rules/infer_pattern_helpers.rs",
                "let mut alias_visited = visited.clone();",
            ),
            allowlist,
        )


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

    def _live_policy_data(self):
        with self.arch_guard.POLICY_PATH.open("rb") as f:
            return self.arch_guard._load_policy_toml(f)

    def test_live_checks_match_policy_entries(self):
        """CHECKS must be loaded from the live TOML policy entries."""
        data = self._live_policy_data()
        pattern_entries = data.get("pattern_checks", [])
        self.assertGreater(len(pattern_entries), 0)
        self.assertEqual(len(self.arch_guard.CHECKS), len(pattern_entries))

    def test_live_manifest_checks_match_policy_entries(self):
        """MANIFEST_CHECKS must be loaded from the live TOML policy entries."""
        data = self._live_policy_data()
        manifest_entries = data.get("manifest_checks", [])
        self.assertGreater(len(manifest_entries), 0)
        self.assertEqual(len(self.arch_guard.MANIFEST_CHECKS), len(manifest_entries))

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
            self.assertGreater(len(name), 0)
            self.assertIsInstance(base, pathlib.Path)
            self.assertIsInstance(pattern, type(self.arch_guard.re.compile("")))
            self.assertIsInstance(excludes, dict)

    def test_manifest_checks_are_tuples_with_correct_shape(self):
        """Each MANIFEST_CHECKS entry must be (name, file_path, compiled_pattern)."""
        for name, file_path, pattern in self.arch_guard.MANIFEST_CHECKS:
            self.assertIsInstance(name, str)
            self.assertGreater(len(name), 0)
            self.assertIsInstance(file_path, pathlib.Path)
            self.assertIsInstance(pattern, type(self.arch_guard.re.compile("")))

    def test_live_policy_names_are_unique(self):
        check_names = [name for name, *_ in self.arch_guard.CHECKS]
        manifest_names = [name for name, *_ in self.arch_guard.MANIFEST_CHECKS]

        self.assertEqual(len(check_names), len(set(check_names)))
        self.assertEqual(len(manifest_names), len(set(manifest_names)))

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
