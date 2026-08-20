"""Contract tests for CI suite metadata."""

import pathlib
import re
import subprocess
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
CI_WORKFLOW = ROOT / ".github" / "workflows" / "ci.yml"


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


def suite_caches(suite: str) -> set[str]:
    result = subprocess.run(
        [
            "bash",
            "-c",
            f"source scripts/ci/suite-metadata.sh; ci_suite_caches {suite}",
        ],
        cwd=ROOT,
        check=True,
        text=True,
        capture_output=True,
    )
    return set(result.stdout.split())


class SuiteMetadataTests(unittest.TestCase):
    def test_github_suites_match_workflow_invocations(self):
        workflow = CI_WORKFLOW.read_text(encoding="utf-8")
        invoked = set(re.findall(r"scripts/ci/github-suite\.sh\s+([A-Za-z0-9_-]+)", workflow))

        self.assertEqual(set(suite_names("github")), invoked)

    def test_github_suites_are_full_suite_entry_points(self):
        self.assertTrue(set(suite_names("github")).issubset(set(suite_names("full"))))

    def test_heavy_helper_suites_are_not_pr_entry_points(self):
        removed_from_pr = {
            "dist-binaries",
            "node-harness-prep",
            "lint",
            "lsp-e2e",
        }

        self.assertTrue(removed_from_pr.isdisjoint(suite_names("github")))
        self.assertTrue(removed_from_pr.issubset(set(suite_names("full"))))

    def test_removed_local_fanout_suites_are_not_entry_points(self):
        removed = {
            "all",
            "full",
            "build",
            "unit-archive",
            "unit-shard",
            "wasm",
            "wasm-web",
            "wasm-all",
            "checker-integration",
            "emit",
            "fourslash",
        }

        self.assertTrue(removed.isdisjoint(suite_names("full")))
        self.assertTrue(removed.isdisjoint(suite_names("github")))
        self.assertTrue(removed.isdisjoint(suite_names("cache")))

    def test_retained_compatibility_observations_stay_addressable(self):
        observations = {
            "conformance",
            "conformance-aggregate",
            "emit-shard",
            "emit-aggregate",
            "fourslash-shard",
            "fourslash-aggregate",
        }
        self.assertTrue(observations.issubset(set(suite_names("github"))))
        self.assertTrue(observations.issubset(set(suite_names("full"))))

    def test_fourslash_shard_initializes_typescript_source(self):
        self.assertIn("typescript-source", suite_caches("fourslash-shard"))


if __name__ == "__main__":
    unittest.main()
