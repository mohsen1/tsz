"""Tests for check-known-failures-growth.py (#15646).

The baseline may only shrink in normal PRs. Growth is authorized in-artifact:
the bootstrap reconcile (base unreconciled) or a reconcile-generation bump on
the marker line in the same diff. The generation can never go backwards
(dropping the marker included), and the file must stay canonical (unique,
sorted, `binary-id::test-name`).
"""

import importlib.util
import pathlib
import re
import subprocess
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
SCRIPT = HERE / "check-known-failures-growth.py"
CHECK_MJS = HERE / "known-failures-check.mjs"

spec = importlib.util.spec_from_file_location("check_known_failures_growth", SCRIPT)
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)

MARKER = guard.RECONCILED_MARKER

UNRECONCILED = "# header\n# more header\n"
RECONCILED_EMPTY = f"# header\n{MARKER}\n"
RECONCILED_TWO = f"# header\n{MARKER} r1\ntsz-a::t::alpha\ntsz-b::t::beta\n"


class MjsDriftPinTests(unittest.TestCase):
    """The marker/format is written by known-failures-check.mjs and judged by
    the python guard; pin the two implementations together."""

    def test_marker_constant_matches_the_mjs(self):
        mjs = CHECK_MJS.read_text(encoding="utf-8")
        match = re.search(r'const RECONCILED_MARKER = "([^"]+)";', mjs)
        self.assertIsNotNone(match, "RECONCILED_MARKER not found in known-failures-check.mjs")
        self.assertEqual(match.group(1), MARKER)

    def test_mjs_rendered_baseline_satisfies_guard_integrity_and_generation(self):
        rendered = subprocess.check_output(
            [
                "node",
                "--input-type=module",
                "-e",
                "import { renderBaseline } from "
                f"'file://{CHECK_MJS}';"
                "process.stdout.write(renderBaseline(['tsz-b::t::y', 'tsz-a::t::x'],"
                " { generation: 3 }));",
            ],
            text=True,
        )
        self.assertEqual(guard.check_integrity(rendered), [])
        self.assertEqual(guard.baseline_generation(rendered), 3)


class ParseAndIntegrityTests(unittest.TestCase):
    def test_parse_entries_skips_comments_and_blanks(self):
        text = "# c\n\n tsz-a::t::x \n# d\ntsz-b::t::y\n"
        self.assertEqual(guard.parse_entries(text), ["tsz-a::t::x", "tsz-b::t::y"])

    def test_generation_parsing(self):
        self.assertEqual(guard.baseline_generation(UNRECONCILED), 0)
        # legacy bare marker reads as generation 1
        self.assertEqual(guard.baseline_generation(RECONCILED_EMPTY), 1)
        self.assertEqual(guard.baseline_generation(f"{MARKER} r7\n"), 7)

    def test_integrity_clean_file(self):
        self.assertEqual(guard.check_integrity(RECONCILED_TWO), [])

    def test_integrity_flags_duplicates(self):
        text = f"{MARKER}\ntsz-a::t::x\ntsz-a::t::x\n"
        self.assertTrue(any("duplicate" in p for p in guard.check_integrity(text)))

    def test_integrity_flags_unsorted(self):
        text = f"{MARKER}\ntsz-b::t::y\ntsz-a::t::x\n"
        self.assertTrue(any("not sorted" in p for p in guard.check_integrity(text)))

    def test_integrity_flags_malformed(self):
        text = f"{MARKER}\nnot-a-nextest-id\n"
        self.assertTrue(any("malformed" in p for p in guard.check_integrity(text)))


class GrowthDecisionTests(unittest.TestCase):
    def test_shrink_is_always_allowed(self):
        head = f"# h\n{MARKER} r1\ntsz-a::t::alpha\n"
        problems, _, added, removed = guard.check_growth(RECONCILED_TWO, head)
        self.assertEqual(problems, [])
        self.assertEqual(added, [])
        self.assertEqual(removed, ["tsz-b::t::beta"])

    def test_growth_without_generation_bump_is_rejected(self):
        head = RECONCILED_TWO + "tsz-c::t::gamma\n"
        problems, _, added, _ = guard.check_growth(RECONCILED_TWO, head)
        self.assertEqual(added, ["tsz-c::t::gamma"])
        self.assertTrue(any("without bumping" in p for p in problems))

    def test_bootstrap_growth_over_unreconciled_base_is_allowed(self):
        problems, notes, added, _ = guard.check_growth(UNRECONCILED, RECONCILED_TWO)
        self.assertEqual(problems, [])
        self.assertEqual(len(added), 2)
        self.assertTrue(any("bootstrap" in n.lower() for n in notes))

    def test_generation_bump_authorizes_growth(self):
        head = RECONCILED_TWO.replace(f"{MARKER} r1", f"{MARKER} r2") + "tsz-c::t::gamma\n"
        problems, notes, _, _ = guard.check_growth(RECONCILED_TWO, head)
        self.assertEqual(problems, [])
        self.assertTrue(any("generation bumped" in n for n in notes))

    def test_generation_can_never_go_backwards(self):
        # dropping the marker entirely is the degenerate case (r1 -> r0)
        head = "# h\ntsz-a::t::alpha\n"
        problems, _, _, _ = guard.check_growth(RECONCILED_TWO, head)
        self.assertTrue(any("went backwards" in p for p in problems))
        # an explicit lower generation is equally rejected, even on shrink
        base_r3 = RECONCILED_TWO.replace(f"{MARKER} r1", f"{MARKER} r3")
        head_r2 = f"# h\n{MARKER} r2\ntsz-a::t::alpha\n"
        problems, _, _, _ = guard.check_growth(base_r3, head_r2)
        self.assertTrue(any("went backwards" in p for p in problems))

    def test_unreconciled_to_unreconciled_edit_is_allowed(self):
        # Pre-bootstrap header edits must not require anything special.
        problems, _, _, _ = guard.check_growth(UNRECONCILED, UNRECONCILED + "# note\n")
        self.assertEqual(problems, [])


class EndToEndGitTests(unittest.TestCase):
    """Drive the CLI against a real (temp) git history. The head baseline is
    the working-tree file; the base comes from a committed ref."""

    def _run_git(self, cwd, *args):
        subprocess.run(
            ["git", *args],
            cwd=cwd,
            check=True,
            capture_output=True,
            env={
                "GIT_AUTHOR_NAME": "t",
                "GIT_AUTHOR_EMAIL": "t@t",
                "GIT_COMMITTER_NAME": "t",
                "GIT_COMMITTER_EMAIL": "t@t",
                "PATH": "/usr/bin:/bin:/usr/local/bin",
                "HOME": cwd,
            },
        )

    def _guard(self, cwd, *args):
        return subprocess.run(
            ["python3", str(SCRIPT), *args],
            cwd=cwd,
            capture_output=True,
            text=True,
            env={"PATH": "/usr/bin:/bin:/usr/local/bin", "HOME": cwd},
        )

    def _repo_with(self, base_text, worktree_text):
        tmp = tempfile.mkdtemp(prefix="kf-growth-")
        self.addCleanup(__import__("shutil").rmtree, tmp, True)
        path = pathlib.Path(tmp) / "scripts" / "ci"
        path.mkdir(parents=True)
        baseline = path / "known-failures.txt"
        self._run_git(tmp, "init", "-q", "-b", "main")
        baseline.write_text(base_text, encoding="utf-8")
        self._run_git(tmp, "add", "-A")
        self._run_git(tmp, "commit", "-q", "-m", "base")
        self._run_git(tmp, "branch", "base-marker")
        baseline.write_text(worktree_text, encoding="utf-8")
        return tmp

    def test_cli_rejects_growth_and_generation_bump_lifts_it(self):
        grown = RECONCILED_TWO + "tsz-c::t::gamma\n"
        repo = self._repo_with(RECONCILED_TWO, grown)
        blocked = self._guard(repo, "--base-ref", "base-marker")
        self.assertEqual(blocked.returncode, 1, blocked.stdout + blocked.stderr)
        bumped = grown.replace(f"{MARKER} r1", f"{MARKER} r2")
        pathlib.Path(repo, "scripts", "ci", "known-failures.txt").write_text(
            bumped, encoding="utf-8"
        )
        allowed = self._guard(repo, "--base-ref", "base-marker")
        self.assertEqual(allowed.returncode, 0, allowed.stdout + allowed.stderr)

    def test_cli_allows_bootstrap_reconcile(self):
        repo = self._repo_with(UNRECONCILED, RECONCILED_TWO)
        result = self._guard(repo, "--base-ref", "base-marker")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_cli_integrity_only_flags_unsorted_worktree(self):
        unsorted_head = f"{MARKER}\ntsz-b::t::y\ntsz-a::t::x\n"
        repo = self._repo_with(UNRECONCILED, unsorted_head)
        result = self._guard(repo, "--integrity-only")
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)

    def test_cli_allow_unavailable_base_warns_and_passes(self):
        repo = self._repo_with(UNRECONCILED, RECONCILED_TWO)
        result = self._guard(
            repo, "--base-ref", "no-such-ref", "--allow-unavailable-base"
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("growth check skipped", result.stderr)


if __name__ == "__main__":
    unittest.main()
