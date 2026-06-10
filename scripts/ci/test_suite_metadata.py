"""Contract tests for CI suite metadata."""

import pathlib
import re
import subprocess
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
CI_WORKFLOW = ROOT / ".github" / "workflows" / "ci.yml"
CLOUDBUILD_CHECKER = ROOT / "scripts" / "cloudbuild" / "cloudbuild-checker-integration.yaml"


def suite_names(scope: str) -> list[str]:
    result = subprocess.run(
        [
            "bash",
            "-c",
            f"source scripts/ci/suite-metadata.sh; ci_suite_names {scope}",
        ],
        cwd=ROOT,
        check=True,
        text=True,
        capture_output=True,
    )
    return [line for line in result.stdout.splitlines() if line]


class SuiteMetadataTests(unittest.TestCase):
    def test_github_suites_match_workflow_invocations(self):
        workflow = CI_WORKFLOW.read_text(encoding="utf-8")
        invoked = set(re.findall(r"scripts/ci/github-suite\.sh\s+([A-Za-z0-9_-]+)", workflow))

        self.assertEqual(set(suite_names("github")), invoked)

    def test_full_suites_add_only_direct_cloudbuild_suite(self):
        cloudbuild = CLOUDBUILD_CHECKER.read_text(encoding="utf-8")
        direct = set(re.findall(r"scripts/ci/gcp-full-ci\.sh\s+([A-Za-z0-9_-]+)", cloudbuild))

        self.assertEqual(set(suite_names("full")), set(suite_names("github")) | direct)
        self.assertEqual(direct, {"checker-integration"})

    def test_removed_local_fanout_suites_are_not_entry_points(self):
        removed = {
            "all",
            "full",
            "build",
            "unit-archive",
            "unit-shard",
            "wasm",
            "wasm-web",
            "emit",
            "emit-aggregate",
            "fourslash",
        }

        self.assertTrue(removed.isdisjoint(suite_names("full")))
        self.assertTrue(removed.isdisjoint(suite_names("github")))


if __name__ == "__main__":
    unittest.main()
