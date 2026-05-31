import json
import os
import pathlib
import stat
import subprocess
import tempfile
import textwrap
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "agents" / "show-goal.sh"


class ShowGoalTests(unittest.TestCase):
    def run_show_goal(self, args, local_goal="# local goal\n", remote_goal="# remote goal\n"):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            temp_root = pathlib.Path(temp_dir)
            fake_repo = temp_root / "repo"
            goal_dir = fake_repo / "docs" / "plan" / "agents"
            goal_dir.mkdir(parents=True)
            (goal_dir / "Studio-F.md").write_text(local_goal, encoding="utf-8")

            fake_bin = temp_root / "bin"
            fake_bin.mkdir()
            calls_log = temp_root / "git-calls.log"
            fake_git = fake_bin / "git"
            fake_git.write_text(
                textwrap.dedent(
                    """\
                    #!/usr/bin/env bash
                    set -euo pipefail

                    printf '%s\n' "$*" >> "$FAKE_GIT_CALLS"

                    if [[ "${1:-}" == "rev-parse" && "${2:-}" == "--show-toplevel" ]]; then
                      printf '%s\n' "$FAKE_REPO"
                      exit 0
                    fi

                    if [[ "${1:-}" == "-C" ]]; then
                      shift 2
                      case "${1:-}" in
                        fetch)
                          exit 0
                          ;;
                        show)
                          if [[ "${2:-}" == "origin/main:docs/plan/agents/Studio-F.md" ]]; then
                            printf '%s' "$FAKE_REMOTE_GOAL"
                            exit 0
                          fi
                          ;;
                      esac
                    fi

                    echo "unexpected git invocation: $*" >&2
                    exit 99
                    """
                ),
                encoding="utf-8",
            )
            fake_git.chmod(fake_git.stat().st_mode | stat.S_IXUSR)

            env = {
                **os.environ,
                "FAKE_GIT_CALLS": str(calls_log),
                "FAKE_REPO": str(fake_repo),
                "FAKE_REMOTE_GOAL": remote_goal,
                "PATH": f"{fake_bin}{os.pathsep}{os.environ['PATH']}",
                "TMPDIR": str(temp_root),
            }

            result = subprocess.run(
                [str(SCRIPT), *args],
                cwd=fake_repo,
                env=env,
                check=True,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            temp_files = sorted(path.name for path in temp_root.glob("tsz-agent-goal.*"))
            calls = calls_log.read_text(encoding="utf-8").splitlines()
            return result.stdout, result.stderr, calls, temp_files

    def test_remote_goal_temp_file_is_cleaned_up(self):
        output, stderr, calls, temp_files = self.run_show_goal(["Studio-F", "--no-fetch"])

        self.assertEqual(output, "# remote goal\n")
        self.assertIn("branch-local docs/plan/agents/Studio-F.md differs", stderr)
        self.assertIn("-C", calls[1])
        self.assertEqual(temp_files, [])

    def test_matching_remote_goal_does_not_warn(self):
        output, stderr, calls, temp_files = self.run_show_goal(
            ["Studio-F", "--no-fetch"],
            local_goal="# same goal\n",
            remote_goal="# same goal\n",
        )

        self.assertEqual(output, "# same goal\n")
        self.assertEqual(stderr, "")
        self.assertIn("-C", calls[1])
        self.assertEqual(temp_files, [])

    def test_local_mode_skips_remote_goal_lookup(self):
        output, stderr, calls, temp_files = self.run_show_goal(["Studio-F", "--local"])

        self.assertEqual(output, "# local goal\n")
        self.assertEqual(stderr, "")
        self.assertEqual(calls, ["rev-parse --show-toplevel"])
        self.assertEqual(temp_files, [])

    def test_json_report_records_remote_goal_source(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            report_path = pathlib.Path(temp_dir) / "goal.json"

            output, stderr, calls, temp_files = self.run_show_goal(
                ["Studio-F", "--no-fetch", "--json-report", str(report_path)]
            )

            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertEqual(output, "# remote goal\n")
            self.assertIn("branch-local docs/plan/agents/Studio-F.md differs", stderr)
            self.assertTrue(report["ok"])
            self.assertEqual("pass", report["status"])
            self.assertEqual("scripts/agents/show-goal.sh", report["generated_by"])
            self.assertEqual("Studio-F", report["agent"])
            self.assertEqual("docs/plan/agents/Studio-F.md", report["goal_path"])
            self.assertEqual("origin/main", report["printed_source"])
            self.assertFalse(report["fetch_attempted"])
            self.assertFalse(report["local_only"])
            self.assertTrue(report["no_fetch"])
            self.assertTrue(report["branch_local_differs"])
            self.assertIn("-C", calls[1])
            self.assertEqual(temp_files, [])

    def test_json_report_records_local_goal_source(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            report_path = pathlib.Path(temp_dir) / "goal.json"

            output, stderr, calls, temp_files = self.run_show_goal(
                ["--json-report", str(report_path), "Studio-F", "--local"]
            )

            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertEqual(output, "# local goal\n")
            self.assertEqual(stderr, "")
            self.assertTrue(report["ok"])
            self.assertEqual("pass", report["status"])
            self.assertEqual("local", report["printed_source"])
            self.assertFalse(report["fetch_attempted"])
            self.assertTrue(report["local_only"])
            self.assertFalse(report["branch_local_differs"])
            self.assertEqual(calls, ["rev-parse --show-toplevel"])
            self.assertEqual(temp_files, [])

    def test_json_report_requires_path(self):
        result = subprocess.run(
            [str(SCRIPT), "Studio-F", "--json-report"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

        self.assertEqual(2, result.returncode)
        self.assertIn("--json-report requires a path", result.stderr)
        self.assertEqual("", result.stdout)


if __name__ == "__main__":
    unittest.main()
