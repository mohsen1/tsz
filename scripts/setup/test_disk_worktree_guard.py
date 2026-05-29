import os
import pathlib
import subprocess
import tempfile
import time
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "setup" / "disk-worktree-guard.sh"


class DiskWorktreeGuardTests(unittest.TestCase):
    def run_git(self, args, cwd):
        return subprocess.run(
            ["git", *args],
            cwd=cwd,
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

    def age_worktree_files(self, worktree, old_timestamp):
        for path in worktree.rglob("*"):
            if path.is_file():
                os.utime(path, (old_timestamp, old_timestamp))

    def make_fake_repo(self, temp_root):
        fake_repo = temp_root / "tsz"
        fake_repo.mkdir()

        script_dir = fake_repo / "scripts" / "setup"
        script_dir.mkdir(parents=True)
        fake_script = script_dir / "disk-worktree-guard.sh"
        fake_script.symlink_to(SCRIPT)

        self.run_git(["init"], fake_repo)
        self.run_git(["config", "user.email", "studio-f@example.invalid"], fake_repo)
        self.run_git(["config", "user.name", "Studio F"], fake_repo)
        self.run_git(["config", "commit.gpgsign", "false"], fake_repo)
        (fake_repo / "README.md").write_text("# fake repo\n", encoding="utf-8")
        self.run_git(["add", "README.md"], fake_repo)
        self.run_git(["commit", "-m", "initial"], fake_repo)
        return fake_repo, fake_script

    def run_guard(self, fake_repo, fake_script):
        env = {
            **os.environ,
            "TSZ_WORKTREE_INACTIVE_HOURS": "1",
        }
        return subprocess.run(
            ["bash", str(fake_script)],
            cwd=fake_repo,
            env=env,
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

    def add_clean_and_dirty_worktrees(self, fake_repo, temp_root):
        clean_worktree = temp_root / "tsz-clean"
        dirty_worktree = temp_root / "tsz-dirty"
        self.run_git(["worktree", "add", "--detach", str(clean_worktree), "HEAD"], fake_repo)
        self.run_git(["worktree", "add", "--detach", str(dirty_worktree), "HEAD"], fake_repo)
        old_timestamp = time.time() - 7200
        self.age_worktree_files(clean_worktree, old_timestamp)
        self.age_worktree_files(dirty_worktree, old_timestamp)
        (dirty_worktree / "untracked.txt").write_text("dirty\n", encoding="utf-8")
        return clean_worktree, dirty_worktree

    def test_dirty_worktrees_are_not_reuse_candidates(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_root = pathlib.Path(temp_dir).resolve()
            fake_repo, fake_script = self.make_fake_repo(temp_root)
            clean_worktree, dirty_worktree = self.add_clean_and_dirty_worktrees(
                fake_repo, temp_root
            )
            result = self.run_guard(fake_repo, fake_script)

            self.assertIn(f"  {clean_worktree} branch=detached:", result.stdout)
            self.assertNotIn(str(dirty_worktree), result.stdout)

    def test_symlinked_repo_parent_still_finds_sibling_worktrees(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_root = pathlib.Path(temp_dir).resolve()
            real_parent = temp_root / "real-parent"
            alias_parent = temp_root / "alias-parent"
            real_parent.mkdir()
            alias_parent.symlink_to(real_parent)

            fake_repo, fake_script = self.make_fake_repo(alias_parent)
            clean_worktree, dirty_worktree = self.add_clean_and_dirty_worktrees(
                fake_repo, alias_parent
            )
            result = self.run_guard(fake_repo, fake_script)

            clean_real = clean_worktree.resolve()
            dirty_real = dirty_worktree.resolve()
            self.assertIn(f"  {clean_real} branch=detached:", result.stdout)
            self.assertNotIn(str(dirty_real), result.stdout)


if __name__ == "__main__":
    unittest.main()
