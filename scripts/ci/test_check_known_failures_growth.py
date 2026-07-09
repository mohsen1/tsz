"""Tests for check-known-failures-growth.py (#15646).

The baseline may only shrink in normal PRs. Growth is allowed exactly twice:
bootstrap (base unreconciled) and an explicit TSZ_KNOWN_FAILURES_ALLOW_GROWTH=1
re-reconcile. The reconciled marker must never be dropped, and the file must
stay canonical (unique, sorted, `binary-id::test-name`).
"""

import importlib.util
import pathlib
import subprocess
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parent / "check-known-failures-growth.py"

spec = importlib.util.spec_from_file_location("check_known_failures_growth", SCRIPT)
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)

MARKER = guard.RECONCILED_MARKER

UNRECONCILED = "# header\n# more header\n"
RECONCILED_EMPTY = f"# header\n{MARKER}\n"
RECONCILED_TWO = f"# header\n{MARKER}\ntsz-a::t::alpha\ntsz-b::t::beta\n"


class ParseAndIntegrityTests(unittest.TestCase):
    def test_parse_entries_skips_comments_and_blanks(self):
        text = "# c\n\n tsz-a::t::x \n# d\ntsz-b::t::y\n"
        self.assertEqual(guard.parse_entries(text), ["tsz-a::t::x", "tsz-b::t::y"])

    def test_reconciled_marker_detection(self):
        self.assertFalse(guard.is_reconciled(UNRECONCILED))
        self.assertTrue(guard.is_reconciled(RECONCILED_EMPTY))

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
        head = f"# h\n{MARKER}\ntsz-a::t::alpha\n"
        problems, added, removed = guard.check_growth(RECONCILED_TWO, head, False)
        self.assertEqual(problems, [])
        self.assertEqual(added, [])
        self.assertEqual(removed, ["tsz-b::t::beta"])

    def test_growth_over_reconciled_base_is_rejected(self):
        head = RECONCILED_TWO + "tsz-c::t::gamma\n"
        problems, added, _ = guard.check_growth(RECONCILED_TWO, head, False)
        self.assertEqual(added, ["tsz-c::t::gamma"])
        self.assertTrue(any("may only shrink" in p for p in problems))

    def test_bootstrap_growth_over_unreconciled_base_is_allowed(self):
        problems, added, _ = guard.check_growth(UNRECONCILED, RECONCILED_TWO, False)
        self.assertEqual(problems, [])
        self.assertEqual(len(added), 2)

    def test_env_escape_allows_deliberate_re_reconcile(self):
        head = RECONCILED_TWO + "tsz-c::t::gamma\n"
        problems, _, _ = guard.check_growth(RECONCILED_TWO, head, True)
        self.assertEqual(problems, [])

    def test_dropping_the_marker_is_rejected_even_on_shrink(self):
        head = "# h\ntsz-a::t::alpha\n"
        problems, _, _ = guard.check_growth(RECONCILED_TWO, head, False)
        self.assertTrue(any("marker was removed" in p for p in problems))

    def test_unreconciled_to_unreconciled_edit_is_allowed(self):
        # Pre-bootstrap header edits must not require the escape.
        problems, _, _ = guard.check_growth(UNRECONCILED, UNRECONCILED + "# note\n", False)
        self.assertEqual(problems, [])


class EndToEndGitTests(unittest.TestCase):
    """Drive the CLI against a real (temp) git history."""

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

    def _guard(self, cwd, *args, env_extra=None):
        env = {
            "PATH": "/usr/bin:/bin:/usr/local/bin",
            "HOME": cwd,
        }
        if env_extra:
            env.update(env_extra)
        return subprocess.run(
            ["python3", str(SCRIPT), *args],
            cwd=cwd,
            capture_output=True,
            text=True,
            env=env,
        )

    def _repo_with(self, base_text, head_text):
        tmp = tempfile.mkdtemp(prefix="kf-growth-")
        path = pathlib.Path(tmp) / "scripts" / "ci"
        path.mkdir(parents=True)
        baseline = path / "known-failures.txt"
        self._run_git(tmp, "init", "-q", "-b", "main")
        baseline.write_text(base_text, encoding="utf-8")
        self._run_git(tmp, "add", "-A")
        self._run_git(tmp, "commit", "-q", "-m", "base")
        self._run_git(tmp, "branch", "base-marker")
        baseline.write_text(head_text, encoding="utf-8")
        self._run_git(tmp, "add", "-A")
        self._run_git(tmp, "commit", "-q", "-m", "head")
        return tmp

    def test_cli_rejects_growth_and_env_escape_lifts_it(self):
        repo = self._repo_with(RECONCILED_TWO, RECONCILED_TWO + "tsz-c::t::gamma\n")
        blocked = self._guard(repo, "--base-ref", "base-marker")
        self.assertEqual(blocked.returncode, 1, blocked.stdout + blocked.stderr)
        allowed = self._guard(
            repo,
            "--base-ref",
            "base-marker",
            env_extra={guard.ALLOW_GROWTH_ENV: "1"},
        )
        self.assertEqual(allowed.returncode, 0, allowed.stdout + allowed.stderr)

    def test_cli_allows_bootstrap_reconcile(self):
        repo = self._repo_with(UNRECONCILED, RECONCILED_TWO)
        result = self._guard(repo, "--base-ref", "base-marker")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_cli_integrity_only_flags_unsorted_head(self):
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
