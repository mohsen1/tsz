import importlib.util
import sys
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).with_name("accepted_regressions.py")
SPEC = importlib.util.spec_from_file_location("accepted_regressions", SCRIPT_PATH)
mod = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = mod
SPEC.loader.exec_module(mod)

normalize = mod.normalize
parse_entries = mod.parse_entries
normalized_entries = mod.normalized_entries
entry_set = mod.entry_set
check_growth = mod.check_growth
entry_comment_blocks = mod.entry_comment_blocks
documented_temporary_additions = mod.documented_temporary_additions
check_integrity = mod.check_integrity


class NormalizeTests(unittest.TestCase):
    def test_repo_relative_path_is_unchanged(self):
        path = "TypeScript/tests/cases/compiler/foo.ts"
        self.assertEqual(normalize(path), path)

    def test_absolute_path_is_sliced_from_typescript_segment(self):
        self.assertEqual(
            normalize("/runner/work/tsz/TypeScript/tests/cases/compiler/foo.ts"),
            "TypeScript/tests/cases/compiler/foo.ts",
        )

    def test_backslashes_are_normalized(self):
        self.assertEqual(
            normalize("TypeScript\\tests\\cases\\compiler\\foo.ts"),
            "TypeScript/tests/cases/compiler/foo.ts",
        )

    def test_path_without_typescript_segment_collapses_separators(self):
        self.assertEqual(normalize("a\\b/c"), "a/b/c")

    def test_mirrors_aggregate_matcher_contract(self):
        # This pins the exact behavior duplicated inline in
        # scripts/ci/lib/gcp-full-ci-conformance.sh. If that copy changes, this
        # assertion must change with it (and vice versa).
        def aggregate_normalize(path):
            parts = path.replace("\\", "/").split("/")
            for i, part in enumerate(parts):
                if part == "TypeScript":
                    return "/".join(parts[i:])
            return "/".join(parts)

        for sample in [
            "TypeScript/tests/cases/compiler/foo.ts",
            "/abs/TypeScript/tests/cases/conformance/bar.ts",
            "TypeScript\\tests\\cases\\compiler\\baz.tsx",
            "no/typescript/here.ts",
        ]:
            self.assertEqual(normalize(sample), aggregate_normalize(sample))


class ParseTests(unittest.TestCase):
    def test_comments_and_blanks_filtered(self):
        text = "# c\n\nTypeScript/tests/cases/compiler/foo.ts\n   \n# d\nbar\n"
        self.assertEqual(
            parse_entries(text),
            ["TypeScript/tests/cases/compiler/foo.ts", "bar"],
        )

    def test_normalized_entries_preserve_order_and_dups(self):
        text = (
            "/abs/TypeScript/tests/cases/compiler/foo.ts\n"
            "TypeScript/tests/cases/compiler/foo.ts\n"
        )
        self.assertEqual(
            normalized_entries(text),
            [
                "TypeScript/tests/cases/compiler/foo.ts",
                "TypeScript/tests/cases/compiler/foo.ts",
            ],
        )

    def test_entry_set_dedups_on_normalized_form(self):
        text = (
            "/abs/TypeScript/tests/cases/compiler/foo.ts\n"
            "TypeScript/tests/cases/compiler/foo.ts\n"
            "TypeScript/tests/cases/compiler/bar.ts\n"
        )
        self.assertEqual(
            entry_set(text),
            frozenset(
                [
                    "TypeScript/tests/cases/compiler/foo.ts",
                    "TypeScript/tests/cases/compiler/bar.ts",
                ]
            ),
        )

    def test_empty_text_is_empty_set(self):
        self.assertEqual(entry_set(""), frozenset())


class GrowthTests(unittest.TestCase):
    def test_no_change_passes(self):
        entries = frozenset(["a.ts", "b.ts"])
        self.assertEqual(check_growth(entries, entries), (frozenset(), frozenset()))

    def test_addition_detected(self):
        added, removed = check_growth(frozenset(["a.ts"]), frozenset(["a.ts", "b.ts"]))
        self.assertEqual(added, frozenset(["b.ts"]))
        self.assertEqual(removed, frozenset())

    def test_removal_only_allowed(self):
        added, removed = check_growth(frozenset(["a.ts", "b.ts"]), frozenset(["a.ts"]))
        self.assertEqual(added, frozenset())
        self.assertEqual(removed, frozenset(["b.ts"]))

    def test_growth_uses_normalized_sets_via_entry_set(self):
        base = entry_set("TypeScript/tests/cases/compiler/foo.ts\n")
        # Same test, non-canonical spelling -> not a real addition.
        head = entry_set("/abs/TypeScript/tests/cases/compiler/foo.ts\n")
        added, removed = check_growth(base, head)
        self.assertEqual(added, frozenset())
        self.assertEqual(removed, frozenset())


class DocumentedTemporaryAdditionTests(unittest.TestCase):
    def test_entry_comment_blocks_use_adjacent_comments(self):
        text = (
            "# detached\n"
            "\n"
            "# tracked by issue #1\n"
            "TypeScript/tests/cases/compiler/foo.ts\n"
        )
        self.assertEqual(
            entry_comment_blocks(text),
            {
                "TypeScript/tests/cases/compiler/foo.ts": [
                    "tracked by issue #1",
                ],
            },
        )

    def test_documented_temporary_addition_is_allowed(self):
        text = (
            "# Tracked by issue #123\n"
            "# Exact evidence: merge-group run 1 failed.\n"
            "# Removal condition: stable after fix.\n"
            "TypeScript/tests/cases/compiler/foo.ts\n"
        )
        documented, rejected = documented_temporary_additions(
            text,
            frozenset(["TypeScript/tests/cases/compiler/foo.ts"]),
        )
        self.assertEqual(
            documented,
            frozenset(["TypeScript/tests/cases/compiler/foo.ts"]),
        )
        self.assertEqual(rejected, frozenset())

    def test_undocumented_temporary_addition_is_rejected(self):
        text = (
            "# Tracked by issue #123\n"
            "# Exact evidence: merge-group run 1 failed.\n"
            "TypeScript/tests/cases/compiler/foo.ts\n"
        )
        documented, rejected = documented_temporary_additions(
            text,
            frozenset(["TypeScript/tests/cases/compiler/foo.ts"]),
        )
        self.assertEqual(documented, frozenset())
        self.assertEqual(
            rejected,
            frozenset(["TypeScript/tests/cases/compiler/foo.ts"]),
        )


class IntegrityTests(unittest.TestCase):
    def test_clean_ledger_has_no_problems(self):
        text = (
            "# header\n"
            "TypeScript/tests/cases/compiler/foo.ts\n"
            "TypeScript/tests/cases/conformance/bar.tsx\n"
        )
        self.assertEqual(check_integrity(text), [])

    def test_duplicate_detected(self):
        text = (
            "TypeScript/tests/cases/compiler/foo.ts\n"
            "TypeScript/tests/cases/compiler/foo.ts\n"
        )
        kinds = [p.kind for p in check_integrity(text)]
        self.assertIn("duplicate", kinds)

    def test_normalized_duplicate_detected(self):
        text = (
            "TypeScript/tests/cases/compiler/foo.ts\n"
            "/abs/TypeScript/tests/cases/compiler/foo.ts\n"
        )
        kinds = [p.kind for p in check_integrity(text)]
        self.assertIn("non-canonical", kinds)
        self.assertIn("duplicate", kinds)

    def test_non_canonical_detected(self):
        text = "/abs/TypeScript/tests/cases/compiler/foo.ts\n"
        problems = check_integrity(text)
        self.assertEqual([p.kind for p in problems], ["non-canonical"])
        self.assertIn("canonical form", problems[0].message)

    def test_malformed_wrong_prefix(self):
        text = "TypeScript/src/compiler/checker.ts\n"
        kinds = [p.kind for p in check_integrity(text)]
        self.assertIn("malformed", kinds)

    def test_malformed_wrong_suffix(self):
        text = "TypeScript/tests/cases/compiler/foo.txt\n"
        kinds = [p.kind for p in check_integrity(text)]
        self.assertIn("malformed", kinds)

    def test_accepts_all_known_suffixes(self):
        text = "".join(
            f"TypeScript/tests/cases/compiler/foo{suffix}\n"
            for suffix in mod.TEST_CASE_SUFFIXES
        )
        self.assertEqual(check_integrity(text), [])

    def test_problem_format_includes_line_number(self):
        text = "\nTypeScript/tests/cases/compiler/foo.txt\n"
        problems = check_integrity(text)
        self.assertEqual(len(problems), 1)
        self.assertTrue(problems[0].format().startswith("line 2: "))


class RealLedgerTests(unittest.TestCase):
    """The checked-in ledger must satisfy the integrity invariant it enforces."""

    def test_checked_in_ledger_is_clean(self):
        ledger = SCRIPT_PATH.parent.parent / "conformance-accepted-regressions.txt"
        text = ledger.read_text(encoding="utf-8")
        problems = check_integrity(text)
        self.assertEqual(
            problems,
            [],
            msg="checked-in ledger violates integrity: "
            + "; ".join(p.format() for p in problems),
        )


if __name__ == "__main__":
    unittest.main()
