import json
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
        (fake_repo / ".gitignore").write_text(".target/\ntarget/\n", encoding="utf-8")
        self.run_git(["add", ".gitignore", "README.md"], fake_repo)
        self.run_git(["commit", "-m", "initial"], fake_repo)
        return fake_repo, fake_script

    def run_guard(self, fake_repo, fake_script, *extra_args, env_overrides=None):
        env = {
            **os.environ,
            "TSZ_WORKTREE_INACTIVE_HOURS": "1",
            **(env_overrides or {}),
        }
        return subprocess.run(
            ["bash", str(fake_script), *extra_args],
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

    def test_low_disk_reports_shortfall(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_root = pathlib.Path(temp_dir).resolve()
            fake_repo, fake_script = self.make_fake_repo(temp_root)

            result = self.run_guard(
                fake_repo,
                fake_script,
                env_overrides={"TSZ_DISK_MIN_FREE_GB": "9999999"},
            )

            self.assertIn("disk_status=low", result.stdout)
            self.assertRegex(result.stdout, r"disk_shortfall_mb=\d+")

    def test_low_disk_reports_cache_pressure_for_clean_inactive_worktrees(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_root = pathlib.Path(temp_dir).resolve()
            fake_repo, fake_script = self.make_fake_repo(temp_root)
            clean_worktree, dirty_worktree = self.add_clean_and_dirty_worktrees(
                fake_repo, temp_root
            )
            (clean_worktree / ".target").mkdir()
            (clean_worktree / ".target" / "cache.bin").write_bytes(b"x" * 1024)
            (dirty_worktree / ".target").mkdir()
            (dirty_worktree / ".target" / "cache.bin").write_bytes(b"x" * 1024)
            old_timestamp = time.time() - 7200
            self.age_worktree_files(clean_worktree, old_timestamp)
            self.age_worktree_files(dirty_worktree, old_timestamp)

            result = self.run_guard(
                fake_repo,
                fake_script,
                env_overrides={"TSZ_DISK_MIN_FREE_GB": "9999999"},
            )

            self.assertIn("cache_pressure_candidates:", result.stdout)
            self.assertIn(f"path={clean_worktree / '.target'}", result.stdout)
            self.assertIn("scope=inactive-clean", result.stdout)
            self.assertNotIn(f"path={dirty_worktree / '.target'}", result.stdout)

    def test_json_report_records_disk_status_and_reuse_candidates(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_root = pathlib.Path(temp_dir).resolve()
            fake_repo, fake_script = self.make_fake_repo(temp_root)
            clean_worktree, dirty_worktree = self.add_clean_and_dirty_worktrees(
                fake_repo, temp_root
            )
            report_path = temp_root / "guard.json"

            result = self.run_guard(fake_repo, fake_script, "--json-report", str(report_path))

            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertIn("disk_free_mb=", result.stdout)
            self.assertEqual("scripts/setup/disk-worktree-guard.sh", report["generated_by"])
            self.assertEqual(str(fake_repo), report["repo_root"])
            self.assertEqual(str(temp_root), report["worktree_parent"])
            self.assertIsInstance(report["ok"], bool)
            self.assertIn(report["status"], ["ok", "low"])
            self.assertIsInstance(report["disk_free_mb"], int)
            self.assertIsInstance(report["disk_shortfall_mb"], int)
            self.assertFalse(report["auto_prune"])
            self.assertIsNone(report["pruned"])
            self.assertIsNone(report["disk_after_auto_prune"])
            self.assertEqual(1, report["reuse_candidate_count"])
            self.assertIsInstance(report["cache_pressure_candidate_count"], int)
            self.assertIsInstance(report["cache_pressure_candidates"], list)
            self.assertEqual(
                {
                    "path": str(clean_worktree),
                    "branch": (
                        "detached:"
                        f"{self.run_git(['rev-parse', '--short=12', 'HEAD'], fake_repo).stdout.strip()}"
                    ),
                    "inactive_hours_min": 1,
                },
                report["reuse_candidates"][0],
            )
            self.assertNotIn(str(dirty_worktree), json.dumps(report))

    def test_json_report_requires_path(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_root = pathlib.Path(temp_dir).resolve()
            fake_repo, fake_script = self.make_fake_repo(temp_root)

            result = subprocess.run(
                ["bash", str(fake_script), "--json-report"],
                cwd=fake_repo,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )

            self.assertEqual(2, result.returncode)
            self.assertIn("--json-report requires a path", result.stderr)


if __name__ == "__main__":
    unittest.main()
