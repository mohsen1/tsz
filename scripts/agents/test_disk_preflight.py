import json
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

    def run_preflight(self, fake_repo, fake_script, *extra_args, env_overrides=None):
        env = {
            **os.environ,
            "TSZ_DISK_MIN_FREE_GB": "1",
            "TSZ_WORKTREE_INACTIVE_HOURS": "1",
            "TSZ_CARGO_CACHE_STUB_MAX_KB": "8",
            **(env_overrides or {}),
        }
        return subprocess.run(
            ["bash", str(fake_script), "Studio-F", *extra_args],
            cwd=fake_repo,
            env=env,
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

    def populate_cache(self, path):
        path.mkdir(parents=True)
        (path / "cache.bin").write_bytes(b"x" * 16 * 1024)

    def test_reports_populated_typescript_and_cache_state(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            temp_root = pathlib.Path(temp_dir).resolve()
            fake_repo, fake_script = self.make_fake_repo(temp_root)
            (fake_repo / "TypeScript" / "tests" / "cases").mkdir(parents=True)
            self.populate_cache(fake_repo / "target")

            result = self.run_preflight(fake_repo, fake_script)

            self.assertIn("agent=Studio-F", result.stdout)
            self.assertIn("git_detached=false", result.stdout)
            self.assertIn("typescript=populated-local-submodule", result.stdout)
            self.assertIn(f"primary={fake_repo} ts-populated", result.stdout)
            self.assertRegex(result.stdout, r"target=present size_kb=\d+")
            self.assertIn("cargo_cache_status=present", result.stdout)
            self.assertRegex(result.stdout, r"cargo_cache_total_kb=\d+")
            self.assertIn("cargo_cache_reuse_sources=0", result.stdout)

    def test_reports_stub_cache_directories_separately(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            temp_root = pathlib.Path(temp_dir).resolve()
            fake_repo, fake_script = self.make_fake_repo(temp_root)
            (fake_repo / "target").mkdir()

            result = self.run_preflight(fake_repo, fake_script)

            self.assertRegex(result.stdout, r"target=stub size_kb=\d+")
            self.assertIn("cargo_cache_status=stub", result.stdout)
            self.assertIn("cargo_cache_reuse_sources=0", result.stdout)

    def test_json_report_records_disk_typescript_and_cache_state(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            temp_root = pathlib.Path(temp_dir).resolve()
            fake_repo, fake_script = self.make_fake_repo(temp_root)
            report_path = temp_root / "preflight.json"
            (fake_repo / "TypeScript" / "tests" / "cases").mkdir(parents=True)
            self.populate_cache(fake_repo / "target")

            result = self.run_preflight(
                fake_repo,
                fake_script,
                "--json-report",
                str(report_path),
            )

            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertIn("agent=Studio-F", result.stdout)
            self.assertTrue(report["ok"])
            self.assertEqual("pass", report["status"])
            self.assertEqual("pass", report["disk_preflight_status"])
            self.assertEqual("Studio-F", report["agent"])
            self.assertEqual(str(fake_repo), report["repo"])
            self.assertEqual(
                self.run_git(["rev-parse", "HEAD"], fake_repo).stdout.strip(),
                report["git_context"]["head"],
            )
            self.assertFalse(report["git_context"]["detached"])
            self.assertTrue(report["git_context"]["branch"])
            self.assertIsNone(report["git_context"]["upstream"])
            self.assertEqual("populated-local-submodule", report["typescript"]["state"])
            self.assertEqual(
                {"path": str(fake_repo), "state": "ts-populated"},
                report["typescript"]["primary"],
            )
            self.assertEqual("present", report["cargo_cache"]["status"])
            self.assertTrue(report["cargo_cache"]["local"]["target"])
            self.assertEqual("present", report["cargo_cache"]["local_status"]["target"])
            self.assertGreater(report["cargo_cache"]["local_size_kb"]["target"], 8)
            self.assertGreater(report["cargo_cache"]["total_size_kb"], 8)
            self.assertFalse(report["cargo_cache"]["local"][".target"])
            self.assertIn("disk_status", report["disk_guard"])
            self.assertTrue(report["disk_guard"]["ok"])
            self.assertNotIn("branch", report["disk_guard"])
            self.assertGreaterEqual(len(report["reusable_worktrees"]), 1)
            self.assertTrue(report["disk_pressure"]["ok"])
            self.assertEqual("ok", report["disk_pressure"]["status"])
            self.assertGreater(report["disk_pressure"]["free_mb"], 0)
            self.assertGreater(report["disk_pressure"]["min_free_mb"], 0)
            self.assertEqual(0, report["disk_pressure"]["shortfall_mb"])
            self.assertEqual([], report["disk_pressure"]["cleanup_ladder"])

    def test_json_report_marks_low_disk_guard_not_ok(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            temp_root = pathlib.Path(temp_dir).resolve()
            fake_repo, fake_script = self.make_fake_repo(temp_root)
            report_path = temp_root / "preflight.json"

            result = self.run_preflight(
                fake_repo,
                fake_script,
                "--json-report",
                str(report_path),
                env_overrides={"TSZ_DISK_MIN_FREE_GB": "9999999"},
            )

            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertFalse(report["ok"])
            self.assertEqual("fail", report["status"])
            self.assertEqual("fail", report["disk_preflight_status"])
            self.assertEqual("low", report["disk_guard"]["disk_status"])
            self.assertIn("disk_shortfall_mb", report["disk_guard"])
            self.assertFalse(report["disk_guard"]["ok"])
            self.assertRegex(result.stdout, r"disk_shortfall_mb=\d+")
            self.assertEqual(1, result.stdout.count("disk_shortfall_mb="))
            self.assertFalse(report["disk_pressure"]["ok"])
            self.assertEqual("low", report["disk_pressure"]["status"])
            self.assertGreater(report["disk_pressure"]["shortfall_mb"], 0)
            self.assertIn(
                "Run scripts/setup/disk-worktree-guard.sh --auto-prune.",
                report["disk_pressure"]["cleanup_ladder"],
            )
            self.assertIn(
                "Use scripts/setup/clean.sh --full only as a deliberate last resort.",
                report["disk_pressure"]["cleanup_ladder"],
            )

    def test_worktree_without_typescript_points_to_link_helper(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            temp_root = pathlib.Path(temp_dir).resolve()
            fake_repo, fake_script = self.make_fake_repo(temp_root)
            (fake_repo / "TypeScript" / "tests" / "cases").mkdir(parents=True)
            self.populate_cache(fake_repo / ".target")

            linked_worktree = temp_root / "tsz-linked"
            self.run_git(
                ["worktree", "add", "--detach", str(linked_worktree), "HEAD"],
                fake_repo,
            )
            linked_script = linked_worktree / "scripts" / "agents" / "disk-preflight.sh"

            result = self.run_preflight(linked_worktree, linked_script)

            self.assertIn("git_detached=true", result.stdout)
            self.assertRegex(result.stdout, r"git_branch=detached:[0-9a-f]+")
            self.assertIn("typescript=missing", result.stdout)
            self.assertIn(f"primary={fake_repo} ts-populated", result.stdout)
            self.assertIn("hint=run scripts/setup/link-ts-submodule.sh", result.stdout)
            self.assertIn("cargo_cache_status=missing", result.stdout)
            self.assertIn("cargo_cache_reuse_sources=1", result.stdout)
            self.assertIn(
                "hint=reuse an existing cached worktree before creating a new build cache",
                result.stdout,
            )

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

    def test_json_report_requires_path(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
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
            self.assertEqual("", result.stdout)


if __name__ == "__main__":
    unittest.main()
