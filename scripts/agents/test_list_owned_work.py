import json
import os
import pathlib
import subprocess
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "agents" / "list-owned-work.sh"


class ListOwnedWorkTests(unittest.TestCase):
    def run_list_owned_work(self, args, prs="", issues=""):
        env = {**os.environ, "FAKE_GH_PRS": prs, "FAKE_GH_ISSUES": issues}
        bootstrap = r"""
gh() {
  case "$1 $2" in
    "pr list") printf "%s" "${FAKE_GH_PRS:-}" ;;
    "issue list") printf "%s" "${FAKE_GH_ISSUES:-}" ;;
    *) echo "unexpected gh invocation: $*" >&2; return 99 ;;
  esac
}
export -f gh
exec "$@"
"""
        return subprocess.run(
            ["/bin/bash", "-c", bootstrap, "bash", str(SCRIPT), *args],
            cwd=ROOT,
            env=env,
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=10,
        )

    def test_clear_owned_work_prints_summary_counters(self):
        result = self.run_list_owned_work(["Studio-F"])

        self.assertIn("## agent:Studio-F", result.stdout)
        self.assertIn("PRs:\n- none", result.stdout)
        self.assertIn("Issues:\n- none", result.stdout)
        self.assertIn("owned_pr_count=0", result.stdout)
        self.assertIn("owned_issue_count=0", result.stdout)
        self.assertIn("owned_work_status=clear", result.stdout)

    def test_active_owned_work_prints_active_summary(self):
        result = self.run_list_owned_work(
            ["Studio-F"],
            prs=(
                "#1 ready first PR https://github.com/mohsen1/tsz/pull/1\n"
                "#2 draft second PR https://github.com/mohsen1/tsz/pull/2\n"
            ),
            issues="#3 issue https://github.com/mohsen1/tsz/issues/3\n",
        )

        self.assertIn("owned_pr_count=2", result.stdout)
        self.assertIn("owned_issue_count=1", result.stdout)
        self.assertIn("owned_work_status=active", result.stdout)

    def test_json_report_records_owned_work_summary(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            report_path = pathlib.Path(temp_dir) / "owned-work.json"

            result = self.run_list_owned_work(
                ["Studio-F", "--json-report", str(report_path)],
                prs="#1 ready first PR https://github.com/mohsen1/tsz/pull/1\n",
                issues="#2 issue https://github.com/mohsen1/tsz/issues/2\n",
            )

            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertIn("## agent:Studio-F", result.stdout)
            self.assertEqual("scripts/agents/list-owned-work.sh", report["generated_by"])
            self.assertEqual("mohsen1/tsz", report["repository"])
            self.assertFalse(report["with_pr_state"])
            self.assertFalse(report["ok"])
            self.assertFalse(report["owned_work_clear"])
            self.assertEqual("active", report["owned_work_status"])
            self.assertEqual(1, report["total_pr_count"])
            self.assertEqual(1, report["total_issue_count"])
            self.assertEqual(2, report["total_owned_count"])
            self.assertEqual(1, len(report["agents"]))
            row = report["agents"][0]
            self.assertEqual("Studio-F", row["agent"])
            self.assertEqual("agent:Studio-F", row["label"])
            self.assertEqual(1, row["pr_count"])
            self.assertEqual(1, row["issue_count"])
            self.assertFalse(row["owned_work_clear"])
            self.assertEqual("active", row["owned_work_status"])
            self.assertEqual(
                ["#1 ready first PR https://github.com/mohsen1/tsz/pull/1"],
                row["prs"],
            )
            self.assertEqual(
                ["#2 issue https://github.com/mohsen1/tsz/issues/2"],
                row["issues"],
            )

    def test_json_report_marks_clear_runway(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            report_path = pathlib.Path(temp_dir) / "owned-work.json"

            self.run_list_owned_work(["Studio-F", "--json-report", str(report_path)])

            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertTrue(report["ok"])
            self.assertTrue(report["owned_work_clear"])
            self.assertEqual("clear", report["owned_work_status"])
            self.assertEqual(0, report["total_pr_count"])
            self.assertEqual(0, report["total_issue_count"])
            self.assertEqual(0, report["total_owned_count"])
            self.assertTrue(report["agents"][0]["owned_work_clear"])
            self.assertEqual("clear", report["agents"][0]["owned_work_status"])

    def test_json_report_requires_path(self):
        result = subprocess.run(
            ["/bin/bash", str(SCRIPT), "--json-report"],
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
