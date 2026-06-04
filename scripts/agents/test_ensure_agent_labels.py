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
    "agent:M1-Opus",
    "agent:M4-A",
    "agent:M4-B",
    "agent:M4-Opus",
    "agent:Studio-A",
    "agent:Studio-B",
    "agent:Studio-C",
    "agent:Studio-Opus",
    "agent:Studio-manager",
]


class EnsureAgentLabelsAuditTests(unittest.TestCase):
    def run_audit_result(self, prs, issues=None, args=None, check=True):
        if issues is None:
            issues = []
        if args is None:
            args = ["--audit"]
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
                      if [[ "$*" != *"--json number,title,isDraft,labels,body,url"* ]]; then
                        echo "expected PR audit to request isDraft and url fields: $*" >&2
                        exit 98
                      fi
                      printf '%s\n' "${FAKE_GH_PRS}"
                      exit 0
                    fi

                    if [[ "${1:-}" == "issue" && "${2:-}" == "list" ]]; then
                      if [[ "$*" != *"--json number,title,labels,url"* ]]; then
                        echo "expected issue audit to request url field: $*" >&2
                        exit 97
                      fi
                      printf '%s\n' "${FAKE_GH_ISSUES}"
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
                "FAKE_GH_ISSUES": json.dumps(issues),
                "PATH": f"{temp_dir}{os.pathsep}{os.environ['PATH']}",
            }

            return subprocess.run(
                [str(SCRIPT), *args],
                cwd=ROOT,
                env=env,
                check=check,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )

    def run_audit_with_prs(self, prs, issues=None):
        return self.run_audit_result(prs, issues=issues).stdout

    def test_audit_separates_intentionally_unassigned_prs(self):
        output = self.run_audit_with_prs(
            [
                {
                    "number": 1,
                    "title": "chore: intentionally unassigned",
                    "isDraft": True,
                    "labels": [],
                    "body": "Coordination Notes\n- No canonical agent lane was assigned.",
                    "url": "https://github.com/tsz-org/tsz/pull/1",
                },
                {
                    "number": 2,
                    "title": "fix: owned",
                    "isDraft": False,
                    "labels": [{"name": "agent:Studio-manager"}],
                    "body": "AgentName: Studio-manager",
                    "url": "https://github.com/tsz-org/tsz/pull/2",
                },
            ]
        )

        self.assertIn("open_prs_missing_agent_label=0", output)
        self.assertIn("open_prs_intentionally_unassigned=1", output)
        self.assertIn("open_ready_prs_intentionally_unassigned=0", output)
        self.assertIn("open_draft_prs_intentionally_unassigned=1", output)
        self.assertIn("agent_label_audit_findings=0", output)
        self.assertIn("agent_label_audit_warnings=0", output)
        self.assertIn("agent_label_audit_status=pass", output)
        self.assertIn("Open PRs Intentionally Unassigned", output)
        self.assertIn("Open Draft PRs Intentionally Unassigned", output)
        self.assertIn(
            "#1 chore: intentionally unassigned https://github.com/tsz-org/tsz/pull/1",
            output,
        )

    def test_audit_still_flags_unexplained_missing_labels(self):
        output = self.run_audit_with_prs(
            [
                {
                    "number": 3,
                    "title": "fix: missing label",
                    "isDraft": False,
                    "labels": [],
                    "body": "AgentName: `Studio-manager`",
                    "url": "https://github.com/tsz-org/tsz/pull/3",
                }
            ]
        )

        self.assertIn("open_prs_missing_agent_label=1", output)
        self.assertIn("open_prs_intentionally_unassigned=0", output)
        self.assertIn("agent_label_audit_findings=1", output)
        self.assertIn("agent_label_audit_warnings=0", output)
        self.assertIn("agent_label_audit_status=fail", output)
        self.assertIn(
            "#3 fix: missing label https://github.com/tsz-org/tsz/pull/3",
            output,
        )
        self.assertIn("AgentName=Studio-manager", output)

    def test_strict_audit_fails_on_actionable_findings(self):
        result = self.run_audit_result(
            [
                {
                    "number": 4,
                    "title": "fix: missing label",
                    "isDraft": False,
                    "labels": [],
                    "body": "AgentName: Studio-manager",
                    "url": "https://github.com/tsz-org/tsz/pull/4",
                }
            ],
            args=["--audit", "--strict"],
            check=False,
        )

        self.assertEqual(1, result.returncode, result.stderr)
        self.assertIn("open_prs_missing_agent_label=1", result.stdout)
        self.assertIn("agent_label_audit_findings=1", result.stdout)
        self.assertIn("agent_label_audit_status=fail", result.stdout)

    def test_strict_audit_allows_intentionally_unassigned_prs(self):
        result = self.run_audit_result(
            [
                {
                    "number": 5,
                    "title": "chore: intentionally unassigned",
                    "isDraft": False,
                    "labels": [],
                    "body": "Coordination Notes\n- No canonical agent lane was assigned.",
                    "url": "https://github.com/tsz-org/tsz/pull/5",
                }
            ],
            args=["--audit", "--strict"],
        )

        self.assertEqual(0, result.returncode)
        self.assertIn("open_prs_intentionally_unassigned=1", result.stdout)
        self.assertIn("open_ready_prs_intentionally_unassigned=1", result.stdout)
        self.assertIn("open_draft_prs_intentionally_unassigned=0", result.stdout)
        self.assertIn("agent_label_audit_findings=0", result.stdout)
        self.assertIn("agent_label_audit_warnings=1", result.stdout)
        self.assertIn("agent_label_audit_status=pass", result.stdout)

    def test_json_report_splits_ready_and_draft_intentionally_unassigned_prs(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            report_path = pathlib.Path(temp_dir) / "reports" / "agent-labels.json"
            result = self.run_audit_result(
                [
                    {
                        "number": 6,
                        "title": "fix: ready unassigned",
                        "isDraft": False,
                        "labels": [],
                        "body": "Coordination Notes\n- No canonical agent lane was assigned.",
                        "url": "https://github.com/tsz-org/tsz/pull/6",
                    },
                    {
                        "number": 7,
                        "title": "fix: draft unassigned",
                        "isDraft": True,
                        "labels": [],
                        "body": "Coordination Notes\n- No canonical agent lane was assigned.",
                        "url": "https://github.com/tsz-org/tsz/pull/7",
                    },
                ],
                args=["--audit", "--json-report", str(report_path)],
            )

            report = json.loads(report_path.read_text(encoding="utf-8"))

        self.assertIn("open_prs_intentionally_unassigned=2", result.stdout)
        self.assertIn("open_ready_prs_intentionally_unassigned=1", result.stdout)
        self.assertIn("open_draft_prs_intentionally_unassigned=1", result.stdout)
        self.assertEqual(
            1,
            report["metrics"]["open_ready_prs_intentionally_unassigned"],
        )
        self.assertEqual(1, report["metrics"]["agent_label_audit_warnings"])
        self.assertEqual(1, report["warning_count"])
        self.assertEqual("warn", report["warning_status"])
        self.assertEqual(
            1,
            report["metrics"]["open_draft_prs_intentionally_unassigned"],
        )
        self.assertEqual(
            [
                {
                    "number": 6,
                    "title": "fix: ready unassigned",
                    "url": "https://github.com/tsz-org/tsz/pull/6",
                    "is_draft": False,
                }
            ],
            report["open_ready_prs_intentionally_unassigned"],
        )
        self.assertEqual(
            [
                {
                    "number": 7,
                    "title": "fix: draft unassigned",
                    "url": "https://github.com/tsz-org/tsz/pull/7",
                    "is_draft": True,
                }
            ],
            report["open_draft_prs_intentionally_unassigned"],
        )

    def test_audit_flags_release_issues_missing_agent_labels(self):
        output = self.run_audit_with_prs(
            [],
            issues=[
                {
                    "number": 10,
                    "title": "bug: missing owner",
                    "labels": [{"name": "bug"}],
                    "url": "https://github.com/tsz-org/tsz/issues/10",
                },
                {
                    "number": 11,
                    "title": "perf: intake context",
                    "labels": [{"name": "performance"}],
                    "url": "https://github.com/tsz-org/tsz/issues/11",
                },
                {
                    "number": 12,
                    "title": "accepted regression owned",
                    "labels": [
                        {"name": "accepted-regression"},
                        {"name": "agent:M1-Opus"},
                    ],
                    "url": "https://github.com/tsz-org/tsz/issues/12",
                },
            ],
        )

        self.assertIn("open_release_issues_missing_agent_label=1", output)
        self.assertIn("Open Release Issues Missing Agent Label", output)
        self.assertIn(
            "#10 bug: missing owner https://github.com/tsz-org/tsz/issues/10",
            output,
        )
        self.assertNotIn("perf: intake context", output)

    def test_audit_flags_issue_agent_label_hygiene(self):
        output = self.run_audit_with_prs(
            [],
            issues=[
                {
                    "number": 20,
                    "title": "issue with too many owners",
                    "labels": [
                        {"name": "agent:M1-A"},
                        {"name": "agent:M1-B"},
                    ],
                    "url": "https://github.com/tsz-org/tsz/issues/20",
                },
                {
                    "number": 21,
                    "title": "issue with generated owner",
                    "labels": [{"name": "agent:claude-sonnet"}],
                    "url": "https://github.com/tsz-org/tsz/issues/21",
                },
            ],
        )

        self.assertIn("open_issues_multiple_agent_labels=1", output)
        self.assertIn("open_issues_noncanonical_agent_label=1", output)
        self.assertIn(
            "#20 agent:M1-A, agent:M1-B issue with too many owners https://github.com/tsz-org/tsz/issues/20",
            output,
        )
        self.assertIn(
            "#21 agent:claude-sonnet issue with generated owner https://github.com/tsz-org/tsz/issues/21",
            output,
        )

    def test_json_report_records_metrics_and_findings(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            report_path = pathlib.Path(temp_dir) / "reports" / "agent-labels.json"
            result = self.run_audit_result(
                [
                    {
                        "number": 30,
                        "title": "fix: missing owner",
                        "isDraft": False,
                        "labels": [],
                        "body": "AgentName: `Studio-manager`",
                        "url": "https://github.com/tsz-org/tsz/pull/30",
                    },
                    {
                        "number": 31,
                        "title": "fix: generated owner",
                        "isDraft": False,
                        "labels": [{"name": "agent:dreamy-runner"}],
                        "body": "AgentName: Studio-manager",
                        "url": "https://github.com/tsz-org/tsz/pull/31",
                    },
                ],
                args=["--audit", "--json-report", str(report_path)],
            )

            self.assertIn("agent_label_audit_status=fail", result.stdout)
            report = json.loads(report_path.read_text(encoding="utf-8"))

        self.assertEqual("fail", report["status"])
        self.assertEqual("fail", report["agent_label_audit_status"])
        self.assertFalse(report["ok"])
        self.assertIn("git_context", report)
        self.assertIsInstance(report["git_context"]["head"], str)
        self.assertIsInstance(report["git_context"]["branch"], str)
        self.assertIsInstance(report["git_context"]["detached"], bool)
        self.assertIn("upstream", report["git_context"])
        self.assertEqual(2, report["metrics"]["agent_label_audit_findings"])
        self.assertEqual(0, report["metrics"]["agent_label_audit_warnings"])
        self.assertEqual(0, report["warning_count"])
        self.assertEqual("clear", report["warning_status"])
        self.assertEqual(1, report["metrics"]["open_prs_missing_agent_label"])
        self.assertEqual(1, report["metrics"]["open_prs_noncanonical_agent_label"])
        self.assertEqual(
            [
                {
                    "number": 30,
                    "title": "fix: missing owner",
                    "url": "https://github.com/tsz-org/tsz/pull/30",
                    "agent_name": "Studio-manager",
                }
            ],
            report["open_prs_missing_agent_label"],
        )
        self.assertEqual(
            [
                {
                    "number": 31,
                    "title": "fix: generated owner",
                    "url": "https://github.com/tsz-org/tsz/pull/31",
                    "agent_labels": ["agent:dreamy-runner"],
                }
            ],
            report["open_prs_noncanonical_agent_label"],
        )

    def test_json_report_records_ok_for_clean_audit(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            report_path = pathlib.Path(temp_dir) / "reports" / "agent-labels.json"
            result = self.run_audit_result(
                [
                    {
                        "number": 32,
                        "title": "fix: owned",
                        "isDraft": False,
                        "labels": [{"name": "agent:Studio-manager"}],
                        "body": "AgentName: Studio-manager",
                        "url": "https://github.com/tsz-org/tsz/pull/32",
                    }
                ],
                args=["--audit", "--json-report", str(report_path)],
            )

            self.assertIn("agent_label_audit_status=pass", result.stdout)
            report = json.loads(report_path.read_text(encoding="utf-8"))

        self.assertTrue(report["ok"])
        self.assertEqual("pass", report["status"])
        self.assertEqual("pass", report["agent_label_audit_status"])
        self.assertEqual(0, report["warning_count"])
        self.assertEqual("clear", report["warning_status"])
        self.assertIn("git_context", report)
        self.assertEqual(0, report["metrics"]["agent_label_audit_findings"])
        self.assertEqual(0, report["metrics"]["agent_label_audit_warnings"])

    def test_json_report_requires_audit_mode(self):
        result = self.run_audit_result(
            [],
            args=["--json-report", "agent-labels.json"],
            check=False,
        )

        self.assertEqual(2, result.returncode)
        self.assertIn("--json-report requires --audit", result.stderr)


if __name__ == "__main__":
    unittest.main()
