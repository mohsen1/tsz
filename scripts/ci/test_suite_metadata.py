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
            "wasm-all",
        }

        self.assertTrue(removed_from_pr.isdisjoint(suite_names("github")))
        self.assertTrue(removed_from_pr.issubset(set(suite_names("full"))))

    def test_no_heavy_github_job_runs_on_pull_request(self):
        """Every heavy suite job must be guarded off pull_request events.

        This is the invariant `test_heavy_helper_suites_are_not_pr_entry_points`
        used to approximate by keeping suites out of the github list. That proxy
        stopped matching the intent once `checker-integration` became its own
        merge_group job: absence from the list no longer means "does not run on
        PRs", the `if:` guard does. Assert the guard directly.
        """
        workflow = CI_WORKFLOW.read_text(encoding="utf-8")
        # Job blocks start at two-space indentation; capture each one's body.
        blocks = re.split(r"\n  (?=[a-z][a-z0-9-]*:\n)", workflow)
        heavy = {
            "unit",
            "checker-integration",
            "conformance",
            "emit",
            "fourslash",
            "clippy",
            "arch-size",
        }
        seen = set()
        for block in blocks:
            match = re.match(r"\s*([a-z][a-z0-9-]*):\n", block)
            if not match or match.group(1) not in heavy:
                continue
            name = match.group(1)
            seen.add(name)
            self.assertIn(
                "github.event_name != 'pull_request'",
                block,
                f"heavy job '{name}' must be guarded off pull_request events",
            )
        self.assertEqual(seen, heavy, "a heavy job disappeared from ci.yml")

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
            "fourslash",
        }

        self.assertTrue(removed.isdisjoint(suite_names("full")))
        self.assertTrue(removed.isdisjoint(suite_names("github")))

    def test_fourslash_shard_initializes_typescript_source(self):
        self.assertIn("typescript-source", suite_caches("fourslash-shard"))


if __name__ == "__main__":
    unittest.main()
