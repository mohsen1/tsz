#!/usr/bin/env python3
"""Focused tests for the standalone pinned TypeScript corpus checkout."""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
RESET_SCRIPT = REPO_ROOT / "scripts/setup/reset-ts-submodule.sh"
LINK_SCRIPT = REPO_ROOT / "scripts/setup/link-ts-submodule.sh"
CALLER_SCRIPTS = [
    REPO_ROOT / "scripts/setup/reset-ts-submodule.sh",
    REPO_ROOT / "scripts/setup/setup.sh",
    REPO_ROOT / "scripts/setup/setup-ts-submodule.sh",
    REPO_ROOT / "scripts/setup/clean.sh",
    REPO_ROOT / "scripts/conformance/conformance.sh",
    REPO_ROOT / "scripts/fourslash/run-fourslash.sh",
]
NO_SUBMODULE_SCRIPTS = [
    *CALLER_SCRIPTS,
    REPO_ROOT / "scripts/setup/link-ts-submodule.sh",
]


def run(
    args: list[str],
    *,
    cwd: Path,
    env: dict[str, str] | None = None,
    check: bool = True,
) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        args,
        cwd=cwd,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if check and result.returncode != 0:
        raise AssertionError(
            f"command failed ({result.returncode}): {args!r}\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return result


class StandaloneCorpusTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(prefix="tsz-corpus-test-")
        self.base = Path(self.temp.name)
        self.remote = self.base / "remote"
        self.checkout = self.base / "checkout"

        run(["git", "init", "--quiet", str(self.remote)], cwd=self.base)
        run(["git", "config", "user.name", "TSZ Test"], cwd=self.remote)
        run(["git", "config", "user.email", "tsz-test@example.invalid"], cwd=self.remote)
        run(["git", "config", "uploadpack.allowFilter", "true"], cwd=self.remote)
        run(
            ["git", "config", "uploadpack.allowReachableSHA1InWant", "true"],
            cwd=self.remote,
        )

        files = {
            "tests/cases/compiler/example.ts": "const value: number = 1;\n",
            "tests/lib/react.d.ts": "interface Fixture {}\n",
            "src/lib/es5.d.ts": "interface Array<T> { length: number; }\n",
            "other/not-needed.txt": "full checkout only\n",
            "README.md": "pinned\n",
        }
        for relative, content in files.items():
            path = self.remote / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")
        run(["git", "add", "."], cwd=self.remote)
        run(["git", "commit", "--quiet", "-m", "pinned corpus"], cwd=self.remote)
        self.pinned_sha = run(
            ["git", "rev-parse", "HEAD"], cwd=self.remote
        ).stdout.strip()

        (self.remote / "README.md").write_text("newer\n", encoding="utf-8")
        run(["git", "add", "README.md"], cwd=self.remote)
        run(["git", "commit", "--quiet", "-m", "newer remote head"], cwd=self.remote)
        self.newer_sha = run(
            ["git", "rev-parse", "HEAD"], cwd=self.remote
        ).stdout.strip()

        (self.checkout / "scripts/setup").mkdir(parents=True)
        (self.checkout / "scripts/ci").mkdir(parents=True)
        (self.checkout / "scripts/conformance").mkdir(parents=True)
        shutil.copy2(RESET_SCRIPT, self.checkout / "scripts/setup/reset-ts-submodule.sh")
        shutil.copy2(
            REPO_ROOT / "scripts/setup/setup-ts-submodule.sh",
            self.checkout / "scripts/setup/setup-ts-submodule.sh",
        )
        self.write_pins(self.pinned_sha, self.pinned_sha)

        self.env = os.environ.copy()
        self.env.update(
            {
                "GIT_ALLOW_PROTOCOL": "file",
                "GIT_TERMINAL_PROMPT": "0",
                "TSZ_TYPESCRIPT_REPOSITORY": self.remote.resolve().as_uri(),
            }
        )

    def tearDown(self) -> None:
        self.temp.cleanup()

    def write_pins(self, ref_sha: str, mapped_sha: str) -> None:
        (self.checkout / "scripts/ci/typescript-submodule-ref").write_text(
            f"{ref_sha}\n", encoding="utf-8"
        )
        (self.checkout / "scripts/conformance/typescript-versions.json").write_text(
            json.dumps(
                {
                    "current": mapped_sha,
                    "mappings": {mapped_sha: {"npm": "7.0.2"}},
                },
                indent=2,
            )
            + "\n",
            encoding="utf-8",
        )

    def reset(
        self,
        *args: str,
        env: dict[str, str] | None = None,
        check: bool = True,
    ) -> subprocess.CompletedProcess[str]:
        return run(
            ["bash", "scripts/setup/reset-ts-submodule.sh", *args],
            cwd=self.checkout,
            env=self.env if env is None else env,
            check=check,
        )

    def commit_remote_file(self, relative: str, content: str, message: str) -> str:
        path = self.remote / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")
        run(["git", "add", relative], cwd=self.remote)
        run(["git", "commit", "--quiet", "-m", message], cwd=self.remote)
        return run(["git", "rev-parse", "HEAD"], cwd=self.remote).stdout.strip()

    def corpus_head(self) -> str:
        return run(
            ["git", "rev-parse", "HEAD"], cwd=self.checkout / "TypeScript"
        ).stdout.strip()

    def test_fresh_checkout_fetches_exact_pin_shallowly(self) -> None:
        result = self.reset()

        self.assertEqual(self.corpus_head(), self.pinned_sha)
        self.assertNotEqual(self.corpus_head(), self.newer_sha)
        self.assertEqual(
            run(
                ["git", "rev-parse", "--is-shallow-repository"],
                cwd=self.checkout / "TypeScript",
            ).stdout.strip(),
            "true",
        )
        self.assertTrue((self.checkout / "TypeScript/tests/cases").is_dir())
        self.assertFalse((self.checkout / ".gitmodules").exists())
        self.assertIn("TypeScript corpus reset to", result.stdout)

    def test_legacy_setup_entrypoint_creates_a_sparse_standalone_checkout(self) -> None:
        run(
            ["bash", "scripts/setup/setup-ts-submodule.sh"],
            cwd=self.checkout,
            env=self.env,
        )

        self.assertEqual(self.corpus_head(), self.pinned_sha)
        self.assertEqual(
            run(
                ["git", "config", "--bool", "core.sparseCheckout"],
                cwd=self.checkout / "TypeScript",
            ).stdout.strip(),
            "true",
        )
        self.assertFalse((self.checkout / ".gitmodules").exists())

    def test_default_mode_disables_a_stale_sparse_checkout(self) -> None:
        run(
            ["bash", "scripts/setup/setup-ts-submodule.sh"],
            cwd=self.checkout,
            env=self.env,
        )
        outside_sparse_set = self.checkout / "TypeScript/other/not-needed.txt"
        self.assertFalse(outside_sparse_set.exists())

        self.reset()

        sparse = run(
            ["git", "config", "--bool", "core.sparseCheckout"],
            cwd=self.checkout / "TypeScript",
            check=False,
        )
        self.assertNotEqual(sparse.stdout.strip(), "true")
        self.assertTrue(outside_sparse_set.is_file())

    def test_dirty_checkout_requires_force_and_force_preserves_ignored_caches(self) -> None:
        self.reset()
        es5 = self.checkout / "TypeScript/src/lib/es5.d.ts"
        es5.write_text("broken\n", encoding="utf-8")
        untracked = self.checkout / "TypeScript/tests/cases/untracked.ts"
        untracked.write_text("untracked\n", encoding="utf-8")
        ignored = self.checkout / "TypeScript/node_modules/keep.txt"
        ignored.parent.mkdir(parents=True)
        ignored.write_text("cache\n", encoding="utf-8")
        run(
            ["git", "-C", "TypeScript", "config", "--local", "status.showUntrackedFiles", "all"],
            cwd=self.checkout,
        )
        exclude = self.checkout / "TypeScript/.git/info/exclude"
        with exclude.open("a", encoding="utf-8") as file:
            file.write("node_modules/\n")

        refused = self.reset(check=False)

        self.assertNotEqual(refused.returncode, 0)
        self.assertEqual(es5.read_text(encoding="utf-8"), "broken\n")
        self.assertTrue(untracked.exists())
        self.assertTrue(ignored.exists())
        self.assertIn("--force-reset", refused.stderr)

        offline_env = self.env.copy()
        offline_env["TSZ_TYPESCRIPT_REPOSITORY"] = (
            self.base / "missing-remote"
        ).resolve().as_uri()
        forced = self.reset("--force-reset", env=offline_env)

        self.assertEqual(es5.read_text(encoding="utf-8"), "interface Array<T> { length: number; }\n")
        self.assertFalse(untracked.exists())
        self.assertTrue(ignored.exists(), "reset should preserve ignored dependency/build caches")
        self.assertIn("Using locally available TypeScript corpus", forced.stdout)

    def test_repository_override_is_used_for_an_existing_origin(self) -> None:
        self.reset()
        new_pin = self.commit_remote_file("new-pin.txt", "new pin\n", "new pin")
        self.write_pins(new_pin, new_pin)
        missing_origin = (self.base / "missing-origin").resolve().as_uri()
        run(
            ["git", "remote", "set-url", "origin", missing_origin],
            cwd=self.checkout / "TypeScript",
        )

        self.reset()

        self.assertEqual(self.corpus_head(), new_pin)
        self.assertTrue((self.checkout / "TypeScript/new-pin.txt").is_file())

    def test_legacy_module_gitdir_is_migrated_into_the_checkout(self) -> None:
        self.reset()
        run(["git", "init", "--quiet"], cwd=self.checkout)
        corpus = self.checkout / "TypeScript"
        legacy_gitdir = self.checkout / ".git/modules/TypeScript"
        legacy_gitdir.parent.mkdir(parents=True)
        shutil.move(str(corpus / ".git"), legacy_gitdir)
        run(
            [
                "git",
                "config",
                "--file",
                str(legacy_gitdir / "config"),
                "core.worktree",
                "../../../TypeScript",
            ],
            cwd=self.checkout,
        )
        (corpus / ".git").write_text(
            "gitdir: ../.git/modules/TypeScript\n", encoding="utf-8"
        )
        self.assertEqual(self.corpus_head(), self.pinned_sha)

        result = self.reset()

        self.assertTrue((corpus / ".git").is_dir())
        self.assertFalse(legacy_gitdir.exists())
        self.assertEqual(self.corpus_head(), self.pinned_sha)
        core_worktree = run(
            ["git", "config", "--get", "core.worktree"],
            cwd=corpus,
            check=False,
        )
        self.assertNotEqual(core_worktree.returncode, 0)
        self.assertIn("Migrated legacy TypeScript module gitdir", result.stdout)

    def test_legacy_worktree_config_gitdir_is_migrated_into_the_checkout(self) -> None:
        self.reset()
        run(["git", "init", "--quiet"], cwd=self.checkout)
        corpus = self.checkout / "TypeScript"
        legacy_gitdir = self.checkout / ".git/modules/TypeScript"
        legacy_gitdir.parent.mkdir(parents=True)
        shutil.move(str(corpus / ".git"), legacy_gitdir)
        run(
            [
                "git",
                "config",
                "--file",
                str(legacy_gitdir / "config"),
                "extensions.worktreeConfig",
                "true",
            ],
            cwd=self.checkout,
        )
        run(
            [
                "git",
                "config",
                "--file",
                str(legacy_gitdir / "config.worktree"),
                "core.worktree",
                "../../../TypeScript",
            ],
            cwd=self.checkout,
        )
        (corpus / ".git").write_text(
            "gitdir: ../.git/modules/TypeScript\n", encoding="utf-8"
        )
        self.assertEqual(self.corpus_head(), self.pinned_sha)

        result = self.reset()

        self.assertTrue((corpus / ".git").is_dir())
        self.assertFalse(legacy_gitdir.exists())
        self.assertEqual(self.corpus_head(), self.pinned_sha)
        core_worktree = run(
            ["git", "config", "--get", "core.worktree"],
            cwd=corpus,
            check=False,
        )
        self.assertNotEqual(core_worktree.returncode, 0)
        self.assertIn("Migrated legacy TypeScript module gitdir", result.stdout)

    def test_unowned_gitfile_is_refused_without_mutation(self) -> None:
        self.reset()
        run(["git", "init", "--quiet"], cwd=self.checkout)
        corpus = self.checkout / "TypeScript"
        external_gitdir = self.base / "external-gitdir"
        shutil.move(str(corpus / ".git"), external_gitdir)
        run(
            [
                "git",
                "config",
                "--file",
                str(external_gitdir / "config"),
                "core.worktree",
                str(corpus),
            ],
            cwd=self.checkout,
        )
        (corpus / ".git").write_text(
            f"gitdir: {external_gitdir}\n", encoding="utf-8"
        )

        result = self.reset(check=False)

        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(external_gitdir.is_dir())
        self.assertTrue((corpus / ".git").is_file())
        self.assertIn("outside the legacy module path", result.stderr)

    def test_non_git_directory_is_never_deleted(self) -> None:
        run(["git", "init", "--quiet"], cwd=self.checkout)
        corpus = self.checkout / "TypeScript"
        corpus.mkdir()
        sentinel = corpus / "user-data.txt"
        sentinel.write_text("keep\n", encoding="utf-8")

        result = self.reset(check=False)

        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(sentinel.exists())
        self.assertIn("refusing to delete it", result.stderr)

    def test_shared_symlink_at_wrong_sha_fails_without_mutation(self) -> None:
        (self.checkout / "TypeScript").symlink_to(self.remote, target_is_directory=True)

        result = self.reset(check=False)

        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(
            run(["git", "rev-parse", "HEAD"], cwd=self.remote).stdout.strip(),
            self.newer_sha,
        )
        self.assertIn("shared symlink", result.stderr)

    def test_disagreeing_pin_files_fail_before_clone(self) -> None:
        self.write_pins(self.pinned_sha, "0" * 40)

        result = self.reset(check=False)

        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.checkout / "TypeScript").exists())
        self.assertIn("pins disagree", result.stderr)


class LinkCorpusSafetyTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(prefix="tsz-link-corpus-test-")
        self.base = Path(self.temp.name)
        self.primary = self.base / "primary"
        self.worktree = self.base / "worktree"
        self.source = self.base / "source-typescript"

        run(["git", "init", "--quiet", str(self.primary)], cwd=self.base)
        self.configure_repo(self.primary)
        (self.primary / "README.md").write_text("primary\n", encoding="utf-8")
        run(["git", "add", "README.md"], cwd=self.primary)
        run(["git", "commit", "--quiet", "-m", "primary"], cwd=self.primary)
        run(
            ["git", "worktree", "add", "--quiet", "-b", "link-test", str(self.worktree)],
            cwd=self.primary,
        )

        run(["git", "init", "--quiet", str(self.source)], cwd=self.base)
        self.configure_repo(self.source)
        case = self.source / "tests/cases/compiler/example.ts"
        case.parent.mkdir(parents=True)
        case.write_text("const value = 1;\n", encoding="utf-8")
        run(["git", "add", "."], cwd=self.source)
        run(["git", "commit", "--quiet", "-m", "source"], cwd=self.source)

    def tearDown(self) -> None:
        self.temp.cleanup()

    @staticmethod
    def configure_repo(path: Path) -> None:
        run(["git", "config", "user.name", "TSZ Test"], cwd=path)
        run(["git", "config", "user.email", "tsz-test@example.invalid"], cwd=path)

    def link(
        self,
        *args: str,
        env: dict[str, str] | None = None,
        check: bool = False,
    ) -> subprocess.CompletedProcess[str]:
        return run(
            ["bash", str(LINK_SCRIPT), "--source", str(self.source), *args],
            cwd=self.worktree,
            env=os.environ.copy() if env is None else env,
            check=check,
        )

    def create_local_git_corpus(self) -> Path:
        corpus = self.worktree / "TypeScript"
        corpus.mkdir()
        run(["git", "init", "--quiet"], cwd=corpus)
        self.configure_repo(corpus)
        tracked = corpus / "tracked.txt"
        tracked.write_text("tracked\n", encoding="utf-8")
        run(["git", "add", "tracked.txt"], cwd=corpus)
        run(["git", "commit", "--quiet", "-m", "local corpus"], cwd=corpus)
        return corpus

    def test_non_git_directory_is_refused_by_default(self) -> None:
        corpus = self.worktree / "TypeScript"
        corpus.mkdir()
        sentinel = corpus / "user-data.txt"
        sentinel.write_text("keep\n", encoding="utf-8")

        result = self.link()

        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(sentinel.is_file())
        self.assertFalse(corpus.is_symlink())
        self.assertIn("not a Git checkout", result.stderr)

    def test_hidden_untracked_files_are_refused_by_default(self) -> None:
        corpus = self.create_local_git_corpus()
        run(["git", "config", "status.showUntrackedFiles", "no"], cwd=corpus)
        untracked = corpus / "untracked.txt"
        untracked.write_text("keep\n", encoding="utf-8")

        result = self.link()

        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(untracked.is_file())
        self.assertIn("untracked files", result.stderr)

    def test_status_failure_is_refused_by_default(self) -> None:
        corpus = self.create_local_git_corpus()
        bad_index = self.base / "bad-index"
        bad_index.mkdir()
        env = os.environ.copy()
        env["GIT_INDEX_FILE"] = str(bad_index)

        result = self.link(env=env)

        self.assertNotEqual(result.returncode, 0)
        self.assertTrue((corpus / "tracked.txt").is_file())
        self.assertIn("cannot inspect", result.stderr)

    def test_force_may_replace_a_non_git_directory(self) -> None:
        corpus = self.worktree / "TypeScript"
        corpus.mkdir()
        (corpus / "discard.txt").write_text("discard\n", encoding="utf-8")

        result = self.link("--force")

        self.assertEqual(result.returncode, 0)
        self.assertTrue(corpus.is_symlink())
        self.assertEqual(corpus.resolve(), self.source.resolve())


class CallerWiringTest(unittest.TestCase):
    def test_active_entrypoints_do_not_execute_git_submodule(self) -> None:
        command = re.compile(r"^[^#\n]*\bgit\s+(?:-[^\s]+\s+)*submodule\b", re.MULTILINE)
        for script in NO_SUBMODULE_SCRIPTS:
            source = script.read_text(encoding="utf-8")
            self.assertIsNone(command.search(source), script)

    def test_callers_share_the_pinned_corpus_helper(self) -> None:
        for script in CALLER_SCRIPTS[1:]:
            source = script.read_text(encoding="utf-8")
            self.assertIn("reset-ts-submodule.sh", source, script)

        helper = RESET_SCRIPT.read_text(encoding="utf-8")
        self.assertIn("--filter=blob:none --no-checkout --depth 1", helper)
        self.assertIn('fetch --filter=blob:none --depth 1 "$FETCH_SOURCE"', helper)
        self.assertIn("--force-reset", helper)

    def test_active_test_callers_request_the_bounded_sparse_corpus(self) -> None:
        conformance = (REPO_ROOT / "scripts/conformance/conformance.sh").read_text(
            encoding="utf-8"
        )
        fourslash = (REPO_ROOT / "scripts/fourslash/run-fourslash.sh").read_text(
            encoding="utf-8"
        )
        self.assertIn('"$reset_helper" --sparse', conformance)
        self.assertIn('reset-ts-submodule.sh" --sparse', fourslash)

    def test_conformance_cleanup_is_scoped_to_test_cases(self) -> None:
        source = (REPO_ROOT / "scripts/conformance/conformance.sh").read_text(
            encoding="utf-8"
        )
        self.assertIn("git clean -xfd -- tests/cases", source)
        self.assertNotIn("git clean -xfd >/dev/null", source)
        self.assertNotIn("git checkout -- .", source)

    def test_repository_has_no_orphan_typescript_gitmodule(self) -> None:
        gitmodules = REPO_ROOT / ".gitmodules"
        if not gitmodules.exists():
            return
        source = gitmodules.read_text(encoding="utf-8")
        self.assertNotIn('[submodule "TypeScript"]', source)


if __name__ == "__main__":
    unittest.main()
