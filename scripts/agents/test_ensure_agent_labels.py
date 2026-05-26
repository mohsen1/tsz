import json
import os
import pathlib
import stat
import subprocess
import tempfile
import textwrap
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "agents" / "ensure-agent-labels.sh"

CANONICAL_AGENT_LABELS = [
    "agent:M1-A",
    "agent:M1-B",
    "agent:M1-C",
    "agent:M1-D",
    "agent:M4-A",
    "agent:M4-B",
    "agent:M4-C",
    "agent:M4-D",
    "agent:Studio-A",
    "agent:Studio-B",
    "agent:Studio-C",
    "agent:Studio-D",
    "agent:Studio-E",
    "agent:Studio-F",
    "agent:Reviewer",
]


class EnsureAgentLabelsAuditTests(unittest.TestCase):
    def run_audit_with_prs(self, prs):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            fake_gh = pathlib.Path(temp_dir) / "gh"
            fake_gh.write_text(
                textwrap.dedent(
                    """\
                    #!/usr/bin/env bash
                    set -euo pipefail

                    if [[ "${1:-}" == "label" && "${2:-}" == "list" ]]; then
                      printf '%s\n' "${FAKE_GH_LABELS}"
                      exit 0
                    fi

                    if [[ "${1:-}" == "pr" && "${2:-}" == "list" ]]; then
                      printf '%s\n' "${FAKE_GH_PRS}"
                      exit 0
                    fi

                    echo "unexpected gh invocation: $*" >&2
                    exit 99
                    """
                ),
                encoding="utf-8",
            )
            fake_gh.chmod(fake_gh.stat().st_mode | stat.S_IXUSR)

            env = {
                **os.environ,
                "FAKE_GH_LABELS": "\n".join(CANONICAL_AGENT_LABELS),
                "FAKE_GH_PRS": json.dumps(prs),
                "PATH": f"{temp_dir}{os.pathsep}{os.environ['PATH']}",
            }

            return subprocess.run(
                [str(SCRIPT), "--audit"],
                cwd=ROOT,
                env=env,
                check=True,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            ).stdout

    def test_audit_separates_intentionally_unassigned_prs(self):
        output = self.run_audit_with_prs(
            [
                {
                    "number": 1,
                    "title": "chore: intentionally unassigned",
                    "labels": [],
                    "body": "Coordination Notes\n- No canonical agent lane was assigned.",
                },
                {
                    "number": 2,
                    "title": "fix: owned",
                    "labels": [{"name": "agent:Studio-F"}],
                    "body": "AgentName: Studio-F",
                },
            ]
        )

        self.assertIn("open_prs_missing_agent_label=0", output)
        self.assertIn("open_prs_intentionally_unassigned=1", output)
        self.assertIn("Open PRs Intentionally Unassigned", output)
        self.assertIn("#1 chore: intentionally unassigned", output)

    def test_audit_still_flags_unexplained_missing_labels(self):
        output = self.run_audit_with_prs(
            [
                {
                    "number": 3,
                    "title": "fix: missing label",
                    "labels": [],
                    "body": "AgentName: Studio-F",
                }
            ]
        )

        self.assertIn("open_prs_missing_agent_label=1", output)
        self.assertIn("open_prs_intentionally_unassigned=0", output)
        self.assertIn("#3 fix: missing label", output)


if __name__ == "__main__":
    unittest.main()
