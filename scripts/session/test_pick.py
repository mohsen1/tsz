"""Behavior-lock unit tests for scripts/session/pick.py helpers."""

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).parent))
import pick  # noqa: E402
from pick import Failure, display_path, resolve_test_source  # noqa: E402


def make_failure(path: str) -> Failure:
    return Failure(path=path, expected=[], actual=[], missing=[], extra=[])


class TestResolveTestSource(unittest.TestCase):
    """`resolve_test_source` finds the test on disk regardless of how the
    snapshot recorded the path. The snapshot stores absolute paths from the
    machine that produced it (e.g. `/tmp/tsz-snap-refresh/TypeScript/...`),
    so a naive `root / failure.path` join fails on every other machine."""

    def test_resolves_via_typescript_segment(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            target = root / "TypeScript" / "tests" / "cases" / "compiler" / "x.ts"
            target.parent.mkdir(parents=True)
            target.write_text("export {};\n")

            failure = make_failure(
                "/tmp/tsz-snap-refresh/TypeScript/tests/cases/compiler/x.ts"
            )
            resolved = resolve_test_source(root, failure)
            self.assertEqual(resolved, target)

    def test_resolves_relative_path(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            target = root / "TypeScript" / "tests" / "cases" / "conformance" / "y.ts"
            target.parent.mkdir(parents=True)
            target.write_text("\n")

            failure = make_failure("TypeScript/tests/cases/conformance/y.ts")
            resolved = resolve_test_source(root, failure)
            self.assertEqual(resolved, target)

    def test_resolves_absolute_existing_path(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "absolute.ts"
            target.write_text("\n")
            other = Path(tempfile.mkdtemp())
            failure = make_failure(str(target))
            resolved = resolve_test_source(other, failure)
            self.assertEqual(resolved, target)

    def test_returns_none_when_missing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            failure = make_failure(
                "/tmp/tsz-snap-refresh/TypeScript/tests/cases/compiler/missing.ts"
            )
            self.assertIsNone(resolve_test_source(Path(tmp), failure))


class TestDisplayPath(unittest.TestCase):
    """`display_path` rewrites the snapshot's foreign absolute path into a
    locally-navigable form when a repo root is provided. Without that, the
    `path:` line printed by the picker points at someone else's worktree
    (e.g. `/Users/<author>/code/tsz/.worktrees/<wt>/TypeScript/...`) and is
    useless for opening or grepping the test on this machine."""

    def test_strips_foreign_prefix_when_root_provided(self) -> None:
        failure = make_failure(
            "/Users/someone/code/tsz/.worktrees/wt/TypeScript/tests/cases/compiler/x.ts"
        )
        self.assertEqual(
            display_path(failure, Path("/home/user/tsz")),
            "TypeScript/tests/cases/compiler/x.ts",
        )

    def test_returns_raw_path_when_no_root(self) -> None:
        raw = "/tmp/tsz-snap-refresh/TypeScript/tests/cases/compiler/x.ts"
        self.assertEqual(display_path(make_failure(raw)), raw)

    def test_returns_raw_path_when_no_typescript_segment(self) -> None:
        raw = "/some/other/path/x.ts"
        self.assertEqual(display_path(make_failure(raw), Path("/home/user/tsz")), raw)

    def test_iteration_var_name_is_not_hardcoded(self) -> None:
        # Defence against the anti-hardcoding directive: the rewrite must
        # depend on the `TypeScript/` segment, not on any contributor name
        # that happens to appear in the absolute prefix.
        for username in ("alice", "BOB", "octocat-2"):
            failure = make_failure(
                f"/Users/{username}/code/tsz/TypeScript/tests/cases/compiler/x.ts"
            )
            self.assertEqual(
                display_path(failure, Path("/home/user/tsz")),
                "TypeScript/tests/cases/compiler/x.ts",
            )


class TestInitTypescriptSubmodule(unittest.TestCase):
    """`init_typescript_submodule` must fall back to a full clone when the
    shallow `--depth 1` clone fails to land a usable working tree.

    The shallow clone is a fast path, but it only works when the pinned
    submodule SHA is reachable from the default-branch tip on origin.
    Pinned commits routinely fall behind upstream, so the agent harness
    must recover automatically rather than leaving contributors with a
    half-cloned `TypeScript/` directory and an opaque `git submodule`
    failure."""

    def _shallow_command(self, root: Path) -> list[str]:
        return [
            "git",
            "-C",
            str(root),
            "submodule",
            "update",
            "--init",
            "--depth",
            "1",
            "TypeScript",
        ]

    def _full_command(self, root: Path) -> list[str]:
        return [
            "git",
            "-C",
            str(root),
            "submodule",
            "update",
            "--init",
            "--recursive",
            "TypeScript",
        ]

    def test_full_clone_runs_when_shallow_fails(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            tests_dir = root / "TypeScript" / "tests"

            shallow_failure = subprocess.CompletedProcess(
                args=[], returncode=128, stderr="fatal: unable to find current revision"
            )

            def _full_clone_creates_tests(*args, **kwargs):
                tests_dir.mkdir(parents=True, exist_ok=True)
                return subprocess.CompletedProcess(args=args[0], returncode=0, stderr="")

            run_calls: list[list[str]] = []

            def _fake_run(cmd, *args, **kwargs):
                run_calls.append(cmd)
                if cmd == self._shallow_command(root):
                    return shallow_failure
                if cmd[:5] == ["git", "-C", str(root), "submodule", "deinit"]:
                    return subprocess.CompletedProcess(args=cmd, returncode=0, stderr="")
                if cmd == self._full_command(root):
                    return _full_clone_creates_tests(cmd, *args, **kwargs)
                return subprocess.CompletedProcess(args=cmd, returncode=0, stderr="")

            with mock.patch.object(pick.subprocess, "run", side_effect=_fake_run):
                pick.init_typescript_submodule(root)

            self.assertIn(self._shallow_command(root), run_calls)
            self.assertIn(self._full_command(root), run_calls)
            self.assertTrue(tests_dir.is_dir())

    def test_full_clone_runs_when_shallow_returns_zero_but_no_tests(self) -> None:
        # Shallow `submodule update` can exit 0 yet leave the working tree
        # empty when an earlier interrupted clone cached partial state.
        # The fallback must still kick in based on the on-disk shape, not
        # the bare exit code, so contributors don't see "missing tests"
        # later in the pipeline.
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            tests_dir = root / "TypeScript" / "tests"

            shallow_success_but_empty = subprocess.CompletedProcess(
                args=[], returncode=0, stderr=""
            )

            def _fake_run(cmd, *args, **kwargs):
                if cmd == self._shallow_command(root):
                    return shallow_success_but_empty
                if cmd[:5] == ["git", "-C", str(root), "submodule", "deinit"]:
                    return subprocess.CompletedProcess(args=cmd, returncode=0, stderr="")
                if cmd == self._full_command(root):
                    tests_dir.mkdir(parents=True, exist_ok=True)
                    return subprocess.CompletedProcess(args=cmd, returncode=0, stderr="")
                return subprocess.CompletedProcess(args=cmd, returncode=0, stderr="")

            with mock.patch.object(pick.subprocess, "run", side_effect=_fake_run):
                pick.init_typescript_submodule(root)

            self.assertTrue(tests_dir.is_dir())

    def test_no_fallback_when_shallow_succeeds(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            tests_dir = root / "TypeScript" / "tests"

            def _fake_run(cmd, *args, **kwargs):
                if cmd == self._shallow_command(root):
                    tests_dir.mkdir(parents=True, exist_ok=True)
                    return subprocess.CompletedProcess(args=cmd, returncode=0, stderr="")
                self.fail(f"unexpected fallback command: {cmd!r}")

            with mock.patch.object(pick.subprocess, "run", side_effect=_fake_run):
                pick.init_typescript_submodule(root)

            self.assertTrue(tests_dir.is_dir())


if __name__ == "__main__":
    unittest.main()
