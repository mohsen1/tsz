import os
import pathlib
import subprocess
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "agents" / "disk-preflight.sh"
GUARD = ROOT / "scripts" / "setup" / "disk-worktree-guard.sh"


class DiskPreflightTests(unittest.TestCase):
    def run_git(self, args, cwd):
        return subprocess.run(
            ["git", *args],
            cwd=cwd,
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

    def make_fake_repo(self, temp_root):
        fake_repo = temp_root / "tsz"
        fake_repo.mkdir()

        agents_dir = fake_repo / "scripts" / "agents"
        setup_dir = fake_repo / "scripts" / "setup"
        agents_dir.mkdir(parents=True)
        setup_dir.mkdir(parents=True)
        fake_script = agents_dir / "disk-preflight.sh"
        fake_guard = setup_dir / "disk-worktree-guard.sh"
        fake_script.symlink_to(SCRIPT)
        fake_guard.symlink_to(GUARD)

        self.run_git(["init"], fake_repo)
        self.run_git(["config", "user.email", "studio-f@example.invalid"], fake_repo)
        self.run_git(["config", "user.name", "Studio F"], fake_repo)
        (fake_repo / "README.md").write_text("# fake repo\n", encoding="utf-8")
        self.run_git(["add", "README.md", "scripts"], fake_repo)
        self.run_git(["commit", "-m", "initial"], fake_repo)

        return fake_repo, fake_script

    def run_preflight(self, fake_repo, fake_script):
        env = {
            **os.environ,
            "TSZ_WORKTREE_INACTIVE_HOURS": "1",
        }
        return subprocess.run(
            ["bash", str(fake_script), "Studio-F"],
            cwd=fake_repo,
            env=env,
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

    def test_reports_populated_typescript_and_cache_state(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            temp_root = pathlib.Path(temp_dir).resolve()
            fake_repo, fake_script = self.make_fake_repo(temp_root)
            (fake_repo / "TypeScript" / "tests" / "cases").mkdir(parents=True)
            (fake_repo / "target").mkdir()

            result = self.run_preflight(fake_repo, fake_script)

            self.assertIn("agent=Studio-F", result.stdout)
            self.assertIn("typescript=populated-local-submodule", result.stdout)
            self.assertIn(f"primary={fake_repo} ts-populated", result.stdout)
            self.assertIn("target=present", result.stdout)

    def test_worktree_without_typescript_points_to_link_helper(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            temp_root = pathlib.Path(temp_dir).resolve()
            fake_repo, fake_script = self.make_fake_repo(temp_root)
            (fake_repo / "TypeScript" / "tests" / "cases").mkdir(parents=True)

            linked_worktree = temp_root / "tsz-linked"
            self.run_git(
                ["worktree", "add", "--detach", str(linked_worktree), "HEAD"],
                fake_repo,
            )
            linked_script = linked_worktree / "scripts" / "agents" / "disk-preflight.sh"

            result = self.run_preflight(linked_worktree, linked_script)

            self.assertIn("typescript=missing", result.stdout)
            self.assertIn(f"primary={fake_repo} ts-populated", result.stdout)
            self.assertIn("hint=run scripts/setup/link-ts-submodule.sh", result.stdout)

    def test_unknown_agent_fails_before_preflight(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            temp_root = pathlib.Path(temp_dir).resolve()
            fake_repo, fake_script = self.make_fake_repo(temp_root)

            result = subprocess.run(
                ["bash", str(fake_script), "Dreamy-F"],
                cwd=fake_repo,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )

            self.assertEqual(1, result.returncode)
            self.assertIn("unknown AgentName: Dreamy-F", result.stderr)
            self.assertEqual("", result.stdout)


if __name__ == "__main__":
    unittest.main()
