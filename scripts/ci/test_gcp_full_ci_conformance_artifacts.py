"""Contract tests for conformance aggregate artifact handoff."""

import pathlib
import re
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
CI_WORKFLOW = ROOT / ".github" / "workflows" / "ci.yml"
GCP_FULL_CI = ROOT / "scripts" / "ci" / "gcp-full-ci.sh"


class ConformanceArtifactHandoffTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.workflow = CI_WORKFLOW.read_text(encoding="utf-8")
        cls.script = GCP_FULL_CI.read_text(encoding="utf-8")

    def function_body(self, name, end_marker):
        start = self.script.index(f"{name}() {{")
        end = self.script.index(end_marker, start)
        return self.script[start:end]

    def test_conformance_shard_artifact_includes_failure_lists(self):
        self.assertIn("name: conformance-shard-${{ matrix.shard }}", self.workflow)
        self.assertRegex(
            self.workflow,
            re.compile(
                r"path:\s*\|\s*\n"
                r"\s+\.ci-metrics/conformance\.json\s*\n"
                r"\s+\.ci-metrics/conformance-failures-\*\.txt",
                re.MULTILINE,
            ),
        )

    def test_shard_writes_failure_list_before_optional_gcs_upload(self):
        body = self.function_body("run_conformance", "\nrun_conformance_aggregate() {")
        failure_write = body.index(
            'grep -a \'^\\(FAIL\\|CRASH\\|TIMEOUT\\) \' "$log_file"',
        )
        upload_block = body.index("# Upload shard result to GCS")
        self.assertLess(failure_write, upload_block)
        failure_write_block = body[failure_write:upload_block]
        self.assertNotIn("XFAIL", failure_write_block)

    def test_aggregate_prefers_artifact_failure_lists_before_gcs(self):
        aggregate = self.function_body(
            "run_conformance_aggregate",
            "\n# Download shard failure lists",
        )
        self.assertIn('local artifact_failure_list="$shard_dir/.ci-metrics/conformance-failures-${shard_name#conformance-shard-}.txt"', aggregate)
        self.assertIn('cp "$artifact_failure_list" "$tmp_dir/failures-shard-${shard_name#conformance-shard-}.txt"', aggregate)
        allowlist = self.function_body(
            "_check_conformance_regression_allowlist",
            "\ndef normalize(path):",
        )
        local_glob = allowlist.index('compgen -G "$tmp_dir/failures-shard-*.txt"')
        gcs_copy = allowlist.index('gsutil -q -m cp "${prefix}/failures-shard-*.txt"')
        self.assertLess(local_glob, gcs_copy)


if __name__ == "__main__":
    unittest.main()
