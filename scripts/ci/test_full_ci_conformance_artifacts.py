"""Contract tests for conformance aggregate artifact handoff."""

import json
import pathlib
import re
import subprocess
import tempfile
import textwrap
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
CI_WORKFLOW = ROOT / ".github" / "workflows" / "ci.yml"
FULL_CI = ROOT / "scripts" / "ci" / "full-ci.sh"
FULL_CI_CONFORMANCE = ROOT / "scripts" / "ci" / "lib" / "full-ci-conformance.sh"


class ConformanceArtifactHandoffTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.workflow = CI_WORKFLOW.read_text(encoding="utf-8")
        cls.script = "\n".join(
            [
                FULL_CI.read_text(encoding="utf-8"),
                FULL_CI_CONFORMANCE.read_text(encoding="utf-8"),
            ],
        )

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
                r"\s+ci-metrics/conformance\.json\s*\n"
                r"\s+ci-metrics/conformance-failures-\*\.txt\s*\n"
                r"\s+ci-metrics/conformance-timings-\*\.json",
                re.MULTILINE,
            ),
        )
        upload_block = self.workflow[
            self.workflow.index("name: conformance-shard-${{ matrix.shard }}") :
        ]
        # Match the key, not a value prefix: "retention-days: 14".index(
        # "retention-days: 1") is 0, which silently truncates the block to ""
        # and makes the assertion below vacuous, and any value not starting
        # with "1" would raise ValueError here instead.
        upload_block = upload_block[: upload_block.index("retention-days:")]
        self.assertNotIn("include-hidden-files", upload_block)

    def test_shard_writes_failure_list_before_artifact_handoff(self):
        body = self.function_body("run_conformance", "\nrun_conformance_aggregate() {")
        failure_write = body.index(
            "awk '/^(FAIL|XFAIL|CRASH|TIMEOUT) / { print $2 }' \"$log_file\"",
        )
        handoff_block = body.index("The workflow uploads conformance.json")
        self.assertLess(failure_write, handoff_block)
        failure_write_block = body[failure_write:handoff_block]
        self.assertIn("XFAIL", failure_write_block)

    def test_weighted_shards_use_checked_in_weights(self):
        body = self.function_body("run_conformance", "\nrun_conformance_aggregate() {")
        self.assertIn(
            'cp scripts/conformance/conformance-shard-weights.json "$shard_weights_file"',
            body,
        )
        self.assertIn("Using checked-in conformance shard weights.", body)
        self.assertNotIn("metrics/latest/conformance-timings.json", body)

    def test_result_reader_partitions_non_runnable_candidates(self):
        reader = self.function_body("read_conformance_results", "\nshow_log_tail() {")
        with tempfile.TemporaryDirectory() as temp_dir:
            temp = pathlib.Path(temp_dir)
            results = temp / "results.txt"
            results.write_text(
                "\n".join(
                    [
                        "PASS pass.ts",
                        "FAIL fail.ts",
                        "XFAIL xfail.ts (accepted)",
                        "CRASH crash.ts",
                        "⏱️ TIMEOUT timeout.ts",
                        "UNSUPPORTED unsupported.ts (typescript-7-unsupported-configuration)",
                        "SKIP skipped.ts",
                    ]
                )
                + "\n",
                encoding="utf-8",
            )
            script = temp / "read.sh"
            script.write_text(
                f"""#!/usr/bin/env bash
set -Eeuo pipefail

{reader}

read_conformance_results "$1"
""",
                encoding="utf-8",
            )
            result = subprocess.run(
                ["bash", str(script), str(results)],
                check=False,
                text=True,
                capture_output=True,
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "1 7 5 1 1")

    def test_shard_plan_reads_explicit_candidate_partition(self):
        body = self.function_body("conformance_shard_plan", "\nrun_conformance() {")
        self.assertIn(".candidates // .total", body)
        self.assertIn(".runnable // .total", body)
        self.assertIn(".unsupported // 0", body)
        self.assertIn(".skipped // 0", body)

    def test_aggregate_uses_artifact_failure_lists_only(self):
        aggregate = self.function_body(
            "run_conformance_aggregate",
            "\n# Download shard failure lists",
        )
        self.assertIn(
            'find "$shard_dir" -maxdepth 4 -name "conformance-failures-${shard_name#conformance-shard-}.txt"',
            aggregate,
        )
        self.assertIn('cp "$artifact_failure_list" "$tmp_dir/failures-shard-${shard_name#conformance-shard-}.txt"', aggregate)
        self.assertIn(
            'find "$shard_dir" -maxdepth 4 -name "conformance-timings-${shard_name#conformance-shard-}.json"',
            aggregate,
        )
        self.assertIn("GitHub Actions artifacts are the only shard handoff.", aggregate)
        self.assertNotIn("gsutil", aggregate)
        allowlist = self.function_body(
            "_check_conformance_regression_allowlist",
            "\ndef normalize(path):",
        )
        self.assertIn('compgen -G "$tmp_dir/failures-shard-*.txt"', allowlist)
        self.assertNotIn("gsutil", allowlist)

    def test_aggregate_accepts_flat_artifact_failure_lists(self):
        aggregate = self.function_body(
            "run_conformance_aggregate",
            "\n# Download shard failure lists",
        )
        allowlist_function = self.function_body(
            "_check_conformance_regression_allowlist",
            "\n# Download per-shard failure lists",
        )
        with tempfile.TemporaryDirectory() as temp_dir:
            temp = pathlib.Path(temp_dir)
            (temp / "ci-metrics").mkdir()
            (temp / "scripts" / "conformance").mkdir(parents=True)
            (temp / "scripts" / "conformance" / "conformance-snapshot.json").write_text(
                '{"summary":{"passed":1,"total_tests":1}}\n',
                encoding="utf-8",
            )
            (temp / "accepted.txt").write_text(
                "TypeScript/tests/cases/compiler/accepted.ts\n",
                encoding="utf-8",
            )
            shard_dir = temp / ".conformance-shards" / "conformance-shard-0"
            shard_dir.mkdir(parents=True)
            shard_dir.joinpath("conformance.json").write_text(
                textwrap.dedent(
                    """\
                    {
                      "passed": 0,
                      "total": 1,
                      "expected_passed": 1,
                      "expected_total": 1
                    }
                    """
                ),
                encoding="utf-8",
            )
            shard_dir.joinpath("conformance-failures-0.txt").write_text(
                "TypeScript/tests/cases/compiler/accepted.ts\n",
                encoding="utf-8",
            )

            script = temp / "aggregate.sh"
            script.write_text(
                f"""#!/usr/bin/env bash
set -Eeuo pipefail

METRICS_DIR=ci-metrics
TSZ_CI_CONFORMANCE_ACCEPTED_FLOOR=0
TSZ_CI_CONFORMANCE_ACCEPTED_REGRESSIONS=accepted.txt
_TSZ_CI_CONFORMANCE_SHARD_COUNT=1

ci_section() {{ :; }}
num_or_zero() {{
  case "${{1:-}}" in
    ''|*[!0-9]*) echo 0 ;;
    *) echo "$1" ;;
  esac
}}
cap_positive_baseline() {{ echo "$1"; }}
publish_latest_metric() {{ :; }}

{aggregate}

{allowlist_function}

run_conformance_aggregate
""",
                encoding="utf-8",
            )
            result = subprocess.run(
                ["bash", str(script)],
                cwd=temp,
                check=False,
                text=True,
                capture_output=True,
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(
            "warning: conformance aggregate below expected only for accepted regressions",
            result.stderr,
        )

    def test_aggregate_uses_snapshot_total_for_coverage_floor(self):
        aggregate = self.function_body(
            "run_conformance_aggregate",
            "\n# Download shard failure lists",
        )
        with tempfile.TemporaryDirectory() as temp_dir:
            temp = pathlib.Path(temp_dir)
            (temp / "ci-metrics").mkdir()
            (temp / "scripts" / "conformance").mkdir(parents=True)
            (temp / "scripts" / "conformance" / "conformance-snapshot.json").write_text(
                '{"summary":{"passed":10,"total_tests":10}}\n',
                encoding="utf-8",
            )
            for shard in range(2):
                shard_dir = (
                    temp
                    / ".conformance-shards"
                    / f"conformance-shard-{shard}"
                    / "ci-metrics"
                )
                shard_dir.mkdir(parents=True)
                shard_dir.joinpath("conformance.json").write_text(
                    textwrap.dedent(
                        """\
                        {
                          "passed": 5,
                          "total": 5,
                          "expected_passed": 0,
                          "expected_total": 10
                        }
                        """
                    ),
                    encoding="utf-8",
                )

            script = temp / "aggregate.sh"
            script.write_text(
                f"""#!/usr/bin/env bash
set -Eeuo pipefail

METRICS_DIR=ci-metrics
TSZ_CI_CONFORMANCE_ACCEPTED_FLOOR=0
_TSZ_CI_CONFORMANCE_SHARD_COUNT=2

ci_section() {{ :; }}
num_or_zero() {{
  case "${{1:-}}" in
    ''|*[!0-9]*) echo 0 ;;
    *) echo "$1" ;;
  esac
}}
cap_positive_baseline() {{ echo "$1"; }}
publish_latest_metric() {{ :; }}

{aggregate}

run_conformance_aggregate
""",
                encoding="utf-8",
            )
            result = subprocess.run(
                ["bash", str(script)],
                cwd=temp,
                check=False,
                text=True,
                capture_output=True,
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Conformance aggregate: 10/10 across 2/2 shards", result.stdout)
        self.assertIn("Conformance expected aggregate: 0/20", result.stdout)

    def test_aggregate_caps_coverage_floor_to_planned_shard_domain(self):
        aggregate = self.function_body(
            "run_conformance_aggregate",
            "\n# Download shard failure lists",
        )
        with tempfile.TemporaryDirectory() as temp_dir:
            temp = pathlib.Path(temp_dir)
            (temp / "ci-metrics").mkdir()
            (temp / "scripts" / "conformance").mkdir(parents=True)
            (temp / "scripts" / "conformance" / "conformance-snapshot.json").write_text(
                '{"summary":{"passed":10,"total_tests":20}}\n',
                encoding="utf-8",
            )
            for shard in range(2):
                shard_dir = (
                    temp
                    / ".conformance-shards"
                    / f"conformance-shard-{shard}"
                    / "ci-metrics"
                )
                shard_dir.mkdir(parents=True)
                shard_dir.joinpath("conformance.json").write_text(
                    textwrap.dedent(
                        """\
                        {
                          "passed": 5,
                          "total": 5,
                          "expected_passed": 0,
                          "expected_total": 5
                        }
                        """
                    ),
                    encoding="utf-8",
                )

            script = temp / "aggregate.sh"
            script.write_text(
                f"""#!/usr/bin/env bash
set -Eeuo pipefail

METRICS_DIR=ci-metrics
TSZ_CI_CONFORMANCE_ACCEPTED_FLOOR=0
_TSZ_CI_CONFORMANCE_SHARD_COUNT=2

ci_section() {{ :; }}
num_or_zero() {{
  case "${{1:-}}" in
    ''|*[!0-9]*) echo 0 ;;
    *) echo "$1" ;;
  esac
}}
cap_positive_baseline() {{ echo "$1"; }}
publish_latest_metric() {{ :; }}

{aggregate}

run_conformance_aggregate
""",
                encoding="utf-8",
            )
            result = subprocess.run(
                ["bash", str(script)],
                cwd=temp,
                check=False,
                text=True,
                capture_output=True,
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Conformance aggregate: 10/10 across 2/2 shards", result.stdout)
        self.assertIn("Conformance expected aggregate: 0/10", result.stdout)

    def test_aggregate_tracks_candidate_partition_and_runnable_pass_rate(self):
        aggregate = self.function_body(
            "run_conformance_aggregate",
            "\n# Download shard failure lists",
        )
        with tempfile.TemporaryDirectory() as temp_dir:
            temp = pathlib.Path(temp_dir)
            (temp / "ci-metrics").mkdir()
            (temp / "scripts" / "conformance").mkdir(parents=True)
            (temp / "scripts" / "conformance" / "conformance-snapshot.json").write_text(
                '{"summary":{"passed":0,"total_tests":4}}\n',
                encoding="utf-8",
            )
            shard_rows = [
                {
                    "passed": 2,
                    "total": 2,
                    "candidates": 4,
                    "runnable": 2,
                    "unsupported": 1,
                    "skipped": 1,
                    "expected_passed": 0,
                    "expected_total": 2,
                    "expected_candidates": 4,
                    "expected_runnable": 2,
                    "expected_unsupported": 1,
                    "expected_skipped": 1,
                },
                {
                    "passed": 1,
                    "total": 2,
                    "candidates": 3,
                    "runnable": 2,
                    "unsupported": 1,
                    "skipped": 0,
                    "expected_passed": 0,
                    "expected_total": 2,
                    "expected_candidates": 3,
                    "expected_runnable": 2,
                    "expected_unsupported": 1,
                    "expected_skipped": 0,
                },
            ]
            for shard, row in enumerate(shard_rows):
                shard_dir = (
                    temp
                    / ".conformance-shards"
                    / f"conformance-shard-{shard}"
                    / "ci-metrics"
                )
                shard_dir.mkdir(parents=True)
                shard_dir.joinpath("conformance.json").write_text(
                    json.dumps(row),
                    encoding="utf-8",
                )

            script = temp / "aggregate.sh"
            script.write_text(
                f"""#!/usr/bin/env bash
set -Eeuo pipefail

METRICS_DIR=ci-metrics
TSZ_CI_CONFORMANCE_ACCEPTED_FLOOR=0
_TSZ_CI_CONFORMANCE_SHARD_COUNT=2

ci_section() {{ :; }}
num_or_zero() {{
  case "${{1:-}}" in
    ''|*[!0-9]*) echo 0 ;;
    *) echo "$1" ;;
  esac
}}
cap_positive_baseline() {{ echo "$1"; }}
publish_latest_metric() {{ :; }}

{aggregate}

run_conformance_aggregate
""",
                encoding="utf-8",
            )
            result = subprocess.run(
                ["bash", str(script)],
                cwd=temp,
                check=False,
                text=True,
                capture_output=True,
            )
            metrics = json.loads(
                (temp / "ci-metrics" / "conformance.json").read_text(encoding="utf-8")
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(
            "Conformance aggregate: 3/4 across 2/2 shards "
            "(7 candidates; 2 unsupported; 1 skipped)",
            result.stdout,
        )
        self.assertEqual(metrics["passed"], 3)
        self.assertEqual(metrics["total"], 4)
        self.assertEqual(metrics["candidates"], 7)
        self.assertEqual(metrics["runnable"], 4)
        self.assertEqual(metrics["unsupported"], 2)
        self.assertEqual(metrics["skipped"], 1)
        self.assertEqual(metrics["pass_rate"], "75.0")

    def test_aggregate_reconstructs_legacy_total_with_skips(self):
        aggregate = self.function_body(
            "run_conformance_aggregate",
            "\n# Download shard failure lists",
        )
        with tempfile.TemporaryDirectory() as temp_dir:
            temp = pathlib.Path(temp_dir)
            (temp / "ci-metrics").mkdir()
            (temp / "scripts" / "conformance").mkdir(parents=True)
            (temp / "scripts" / "conformance" / "conformance-snapshot.json").write_text(
                '{"summary":{"passed":1,"total_tests":1}}\n',
                encoding="utf-8",
            )
            shard_dir = temp / ".conformance-shards" / "conformance-shard-0"
            shard_dir.mkdir(parents=True)
            shard_dir.joinpath("conformance.json").write_text(
                '{"passed":1,"total":2,"skipped":1,"expected_passed":0}\n',
                encoding="utf-8",
            )
            script = temp / "aggregate.sh"
            script.write_text(
                f"""#!/usr/bin/env bash
set -Eeuo pipefail

METRICS_DIR=ci-metrics
TSZ_CI_CONFORMANCE_ACCEPTED_FLOOR=0
_TSZ_CI_CONFORMANCE_SHARD_COUNT=1

ci_section() {{ :; }}
num_or_zero() {{
  case "${{1:-}}" in
    ''|*[!0-9]*) echo 0 ;;
    *) echo "$1" ;;
  esac
}}
cap_positive_baseline() {{ echo "$1"; }}
publish_latest_metric() {{ :; }}

{aggregate}

run_conformance_aggregate
""",
                encoding="utf-8",
            )
            result = subprocess.run(
                ["bash", str(script)],
                cwd=temp,
                check=False,
                text=True,
                capture_output=True,
            )
            metrics = json.loads(
                (temp / "ci-metrics" / "conformance.json").read_text(encoding="utf-8")
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(metrics["candidates"], 2)
        self.assertEqual(metrics["runnable"], 1)
        self.assertEqual(metrics["total"], 1)
        self.assertEqual(metrics["skipped"], 1)

    def test_allowlist_accepts_all_conformance_failure_statuses(self):
        allowlist_function = self.function_body(
            "_check_conformance_regression_allowlist",
            "\n# Download per-shard failure lists",
        )
        with tempfile.TemporaryDirectory() as temp_dir:
            temp = pathlib.Path(temp_dir)
            script = temp / "check.sh"
            allowlist = temp / "accepted.txt"
            log_file = temp / "conformance.log"
            allowlist.write_text(
                "\n".join(
                    [
                        "TypeScript/tests/cases/compiler/crash.ts",
                        "TypeScript/tests/cases/compiler/fail.ts",
                        "TypeScript/tests/cases/compiler/timeout.ts",
                        "TypeScript/tests/cases/compiler/xfail.ts",
                    ]
                )
                + "\n",
                encoding="utf-8",
            )
            log_file.write_text(
                "\n".join(
                    [
                        "PASS TypeScript/tests/cases/compiler/pass.ts",
                        "FAIL TypeScript/tests/cases/compiler/fail.ts | expected:[TS2322] actual:[]",
                        "XFAIL TypeScript/tests/cases/compiler/xfail.ts (accepted)",
                        "CRASH TypeScript/tests/cases/compiler/crash.ts",
                        "TIMEOUT TypeScript/tests/cases/compiler/timeout.ts",
                    ]
                )
                + "\n",
                encoding="utf-8",
            )
            script.write_text(
                f"""#!/usr/bin/env bash
set -Eeuo pipefail

{allowlist_function}

tmp_dir="$1"
log_file="$2"
export TSZ_CI_CONFORMANCE_ACCEPTED_REGRESSIONS="$3"

awk '/^(FAIL|XFAIL|CRASH|TIMEOUT) / {{ print $2 }}' "$log_file" \\
  | sort -u > "$tmp_dir/failures-shard-0.txt"

_check_conformance_regression_allowlist "$tmp_dir" "" 4
""",
                encoding="utf-8",
            )
            result = subprocess.run(
                ["bash", str(script), temp_dir, str(log_file), str(allowlist)],
                check=False,
                text=True,
                capture_output=True,
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(
            "accepted regressions: 4/4 listed tests currently failing",
            result.stderr,
        )


if __name__ == "__main__":
    unittest.main()
