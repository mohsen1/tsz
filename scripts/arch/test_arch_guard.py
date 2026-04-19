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


class ArchGuardCommentStrippingTests(unittest.TestCase):
    def setUp(self):
        self.arch_guard = load_arch_guard_module()

    def test_find_matches_ignores_block_comments_when_requested(self):
        pattern = self.arch_guard.re.compile(r"\bTypeKey::")
        text = "\n".join(
            [
                "/* TypeKey::Foo should be ignored */",
                "let x = 1;",
            ]
        )
        hits = self.arch_guard.find_matches(
            text,
            pattern,
            "crates/tsz-checker/src/foo.rs",
            {"ignore_comment_lines": True},
        )
        self.assertEqual(hits, [])

    def test_find_matches_preserves_real_code_hits_with_inline_block_comments(self):
        pattern = self.arch_guard.re.compile(r"\bTypeKey::")
        text = "\n".join(
            [
                "let ok = true; /* TypeKey::CommentOnly */",
                "let value = TypeKey::Real;",
            ]
        )
        hits = self.arch_guard.find_matches(
            text,
            pattern,
            "crates/tsz-checker/src/foo.rs",
            {"ignore_comment_lines": True},
        )
        self.assertEqual(hits, [2])

    def test_find_matches_keeps_comment_hits_without_ignore_flag(self):
        pattern = self.arch_guard.re.compile(r"\bTypeKey::")
        text = "/* TypeKey::Foo should match when comments are not ignored */"
        hits = self.arch_guard.find_matches(
            text,
            pattern,
            "crates/tsz-checker/src/foo.rs",
            {},
        )
        self.assertEqual(hits, [1])


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


if __name__ == "__main__":
    unittest.main()
