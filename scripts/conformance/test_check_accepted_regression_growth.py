import importlib.util
import sys
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).with_name("check-accepted-regression-growth.py")
SPEC = importlib.util.spec_from_file_location("check_accepted_regression_growth", SCRIPT_PATH)
mod = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = mod
SPEC.loader.exec_module(mod)

check_growth = mod.check_growth


def _parse(text: str) -> frozenset[str]:
    """Parse accepted-regression text directly (mirrors load_entries logic)."""
    entries: set[str] = set()
    for line in text.splitlines():
        stripped = line.strip()
        if stripped and not stripped.startswith("#"):
            entries.add(stripped)
    return frozenset(entries)


class CheckAcceptedRegressionGrowthTests(unittest.TestCase):
    # ------------------------------------------------------------------ #
    # check_growth logic                                                    #
    # ------------------------------------------------------------------ #

    def test_no_change_passes(self):
        entries = frozenset(["TypeScript/tests/cases/foo.ts", "TypeScript/tests/cases/bar.ts"])
        added, removed = check_growth(entries, entries)
        self.assertEqual(added, frozenset())
        self.assertEqual(removed, frozenset())

    def test_removal_only_passes(self):
        base = frozenset(["foo.ts", "bar.ts"])
        head = frozenset(["foo.ts"])
        added, removed = check_growth(base, head)
        self.assertEqual(added, frozenset())
        self.assertEqual(removed, frozenset(["bar.ts"]))

    def test_addition_detected(self):
        base = frozenset(["foo.ts"])
        head = frozenset(["foo.ts", "new-regression.ts"])
        added, removed = check_growth(base, head)
        self.assertEqual(added, frozenset(["new-regression.ts"]))
        self.assertEqual(removed, frozenset())

    def test_addition_and_removal_detected_separately(self):
        base = frozenset(["old.ts", "kept.ts"])
        head = frozenset(["new.ts", "kept.ts"])
        added, removed = check_growth(base, head)
        self.assertEqual(added, frozenset(["new.ts"]))
        self.assertEqual(removed, frozenset(["old.ts"]))

    def test_empty_sets_pass(self):
        added, removed = check_growth(frozenset(), frozenset())
        self.assertEqual(added, frozenset())
        self.assertEqual(removed, frozenset())

    def test_empty_base_nonempty_head_detected(self):
        added, removed = check_growth(frozenset(), frozenset(["new.ts"]))
        self.assertEqual(added, frozenset(["new.ts"]))
        self.assertEqual(removed, frozenset())

    def test_multiple_additions_all_reported(self):
        base = frozenset(["kept.ts"])
        head = frozenset(["kept.ts", "added-1.ts", "added-2.ts"])
        added, removed = check_growth(base, head)
        self.assertEqual(added, frozenset(["added-1.ts", "added-2.ts"]))
        self.assertEqual(removed, frozenset())

    # ------------------------------------------------------------------ #
    # Entry parsing (comment/blank filtering)                              #
    # ------------------------------------------------------------------ #

    def test_comments_filtered(self):
        text = "# this is a comment\n\nfoo.ts\n# another comment\nbar.ts\n"
        entries = _parse(text)
        self.assertEqual(entries, frozenset(["foo.ts", "bar.ts"]))

    def test_blank_lines_filtered(self):
        text = "foo.ts\n\n\n   \nbar.ts"
        entries = _parse(text)
        self.assertEqual(entries, frozenset(["foo.ts", "bar.ts"]))

    def test_empty_file_produces_empty_set(self):
        self.assertEqual(_parse(""), frozenset())

    def test_whitespace_only_file_produces_empty_set(self):
        self.assertEqual(_parse("   \n\n  \n"), frozenset())


if __name__ == "__main__":
    unittest.main()
