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
            return result.stdout, calls, temp_files

    def test_remote_goal_temp_file_is_cleaned_up(self):
        output, calls, temp_files = self.run_show_goal(["Studio-F", "--no-fetch"])

        self.assertEqual(output, "# remote goal\n")
        self.assertIn("-C", calls[1])
        self.assertEqual(temp_files, [])

    def test_local_mode_skips_remote_goal_lookup(self):
        output, calls, temp_files = self.run_show_goal(["Studio-F", "--local"])

        self.assertEqual(output, "# local goal\n")
        self.assertEqual(calls, ["rev-parse --show-toplevel"])
        self.assertEqual(temp_files, [])


if __name__ == "__main__":
    unittest.main()
