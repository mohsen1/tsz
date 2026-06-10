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
        self.run_git(["config", "user.email", "studio-manager@example.invalid"], fake_repo)
        self.run_git(["config", "user.name", "Studio Manager"], fake_repo)
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
            ["bash", str(fake_script), "studio", *extra_args],
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

            self.assertIn("agent=studio", result.stdout)
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
            self.assertIn("agent=studio", result.stdout)
            self.assertTrue(report["ok"])
            self.assertEqual("pass", report["status"])
            self.assertEqual("pass", report["disk_preflight_status"])
            self.assertEqual("studio", report["agent"])
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
            self.assertEqual(0, report["disk_pressure"]["sister_reuse_candidate_count"])
            self.assertEqual([], report["disk_pressure"]["sister_reuse_candidates"])
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
            self.assertFalse(report["git_context"]["dirty"])
            self.assertEqual(0, report["git_context"]["dirty_files"])
            self.assertEqual(0, report["git_context"]["untracked_files"])
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

    def test_json_report_records_dirty_git_state(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            temp_root = pathlib.Path(temp_dir).resolve()
            fake_repo, fake_script = self.make_fake_repo(temp_root)
            report_path = temp_root / "preflight.json"
            (fake_repo / "README.md").write_text("# fake repo\n\nchanged\n", encoding="utf-8")
            (fake_repo / "scratch.txt").write_text("scratch\n", encoding="utf-8")

            result = self.run_preflight(
                fake_repo,
                fake_script,
                "--json-report",
                str(report_path),
            )

            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertIn("git_dirty=true", result.stdout)
            self.assertIn("git_dirty_files=2", result.stdout)
            self.assertIn("git_untracked_files=1", result.stdout)
            self.assertTrue(report["git_context"]["dirty"])
            self.assertEqual(2, report["git_context"]["dirty_files"])
            self.assertEqual(1, report["git_context"]["untracked_files"])

    def test_json_report_records_sister_reuse_candidates(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            temp_root = pathlib.Path(temp_dir).resolve()
            fake_repo, fake_script = self.make_fake_repo(temp_root)
            fake_guard = fake_repo / "scripts" / "setup" / "disk-worktree-guard.sh"
            fake_guard.unlink()
            candidate_path = temp_root / "tsz-candidate"
            fake_guard.write_text(
                "#!/usr/bin/env bash\n"
                "echo 'disk_free_gb=42 path=/tmp'\n"
                "echo 'disk_free_mb=43008'\n"
                "echo 'disk_status=ok min_free_gb=1'\n"
                "echo 'sister_worktree_reuse_candidates:'\n"
                f"echo '  {candidate_path} branch=refs/heads/candidate inactive_hours>=4'\n",
                encoding="utf-8",
            )
            fake_guard.chmod(0o755)
            report_path = temp_root / "preflight.json"

            self.run_preflight(
                fake_repo,
                fake_script,
                "--json-report",
                str(report_path),
            )

            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertEqual(1, report["disk_pressure"]["sister_reuse_candidate_count"])
            self.assertEqual(
                {
                    "path": str(candidate_path),
                    "branch": "refs/heads/candidate",
                    "inactive_hours_min": 4,
                },
                report["disk_pressure"]["sister_reuse_candidates"][0],
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
            report_path = temp_root / "preflight.json"

            result = self.run_preflight(
                linked_worktree,
                linked_script,
                "--json-report",
                str(report_path),
            )
            report = json.loads(report_path.read_text(encoding="utf-8"))

            self.assertIn("git_detached=true", result.stdout)
            self.assertRegex(result.stdout, r"git_branch=detached:[0-9a-f]+")
            self.assertIn("typescript=missing", result.stdout)
            self.assertIn(f"primary={fake_repo} ts-populated", result.stdout)
            self.assertIn(f"source={fake_repo} ts-populated", result.stdout)
            self.assertIn(
                {
                    "kind": "source",
                    "path": str(fake_repo),
                    "state": "ts-populated",
                },
                report["typescript"]["reuse_sources"],
            )
            self.assertIn("hint=run scripts/setup/link-ts-submodule.sh", result.stdout)
            self.assertIn("cargo_cache_status=missing", result.stdout)
            self.assertIn("cargo_cache_reuse_sources=1", result.stdout)
            self.assertIn(
                "hint=reuse an existing cached worktree before creating a new build cache",
                result.stdout,
            )

    def test_free_form_label_is_echoed(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            temp_root = pathlib.Path(temp_dir).resolve()
            fake_repo, fake_script = self.make_fake_repo(temp_root)

            result = subprocess.run(
                ["bash", str(fake_script), "studio"],
                cwd=fake_repo,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )

            self.assertEqual(0, result.returncode)
            self.assertIn("agent=studio", result.stdout)

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
