import subprocess
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("conformance.sh")


class SnapshotAccountingContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.script = SCRIPT.read_text(encoding="utf-8")
        cls.provenance = SCRIPT.with_name("snapshot-provenance.py").read_text(
            encoding="utf-8"
        )

    def test_snapshot_completeness_checks_candidates_and_runnable_rows(self):
        self.assertGreaterEqual(self.script.count("validate-runner-output.py"), 2)
        self.assertIn('d.get(\'recorded_candidates\', 0)', self.script)
        self.assertIn('d.get(\'recorded_runnable\', 0)', self.script)
        self.assertIn(
            '[ "$recorded_results" -ne "$candidate_tests" ]',
            self.script,
        )
        self.assertIn(
            '[ "$recorded_runnable" -ne "$total_tests" ]',
            self.script,
        )
        self.assertIn(
            "Incomplete candidate coverage cannot be saved, including with --force",
            self.script,
        )

    def test_snapshot_summary_persists_full_partition(self):
        for field in (
            "'candidates': candidates",
            "'total_tests': runnable",
            "'runnable': runnable",
            "'unsupported': unsupported",
            "'skipped': skipped",
        ):
            with self.subTest(field=field):
                self.assertIn(field, self.script)
        self.assertIn(
            "candidates != runnable + unsupported + skipped",
            self.script,
        )

    def test_detail_generation_is_required(self):
        detail_call = self.script.index("build-snapshot-detail.py")
        analyze_call = self.script.index("analyze-conformance.py", detail_call)
        block = self.script[detail_call:analyze_call]
        self.assertIn("failed to build conformance detail snapshot", block)
        self.assertNotIn("|| true", block)

    def test_snapshot_uses_exactly_one_canonical_invocation(self):
        snapshot = self.script[
            self.script.index("snapshot_tests()") : self.script.index("# Parse arguments")
        ]
        self.assertIn("first and only canonical invocation", snapshot)
        self.assertNotIn("max_attempts", snapshot)
        self.assertNotIn("Retrying", snapshot)
        self.assertNotIn("for attempt", snapshot)

    def test_regular_run_propagates_runner_status_through_tee(self):
        run = self.script[
            self.script.index("run_tests()") : self.script.index("analyze_tests()")
        ]
        self.assertIn("runner_status=${PIPESTATUS[0]}", run)
        self.assertIn('return "$runner_status"', run)
        runner_pipeline = run[
            run.index("$RUNNER_BIN") : run.index("# Never overwrite")
        ]
        self.assertNotIn("|| true", runner_pipeline)

    def test_snapshot_rejects_every_subset_and_custom_corpus(self):
        for token in (
            "--filter",
            "--max",
            "-m",
            "--offset",
            "-o",
            "--shard",
            "--no-cache",
            "CUSTOM_TEST_DIR",
        ):
            with self.subTest(token=token):
                self.assertIn(token, self.script)
        self.assertIn("tracked snapshots reject subset/custom runner argument", self.script)
        self.assertLess(
            self.script.index("validate_snapshot_selection || exit 1"),
            self.script.index('case "$COMMAND" in'),
        )

    def test_snapshot_short_subset_flags_fail_before_any_setup(self):
        for arguments in (("-m", "1"), ("-o", "1")):
            with self.subTest(arguments=arguments):
                result = subprocess.run(
                    ["bash", str(SCRIPT), "snapshot", *arguments],
                    cwd=SCRIPT.parents[2],
                    capture_output=True,
                    text=True,
                    check=False,
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(
                    "tracked snapshots reject subset/custom runner argument",
                    result.stderr,
                )
                self.assertNotIn("TypeScript corpus", result.stdout)

    def test_empty_forwarded_arrays_are_nounset_safe(self):
        expected_safe_expansions = {
            "REMAINING_ARGS": 8,
            "runner_flags": 1,
            "extra_args": 3,
        }
        script_without_safe_expansions = self.script
        for name, expected_count in expected_safe_expansions.items():
            unsafe = f'"${{{name}[@]}}"'
            safe = f'"${{{name}[@]+"${{{name}[@]}}"}}"'
            self.assertEqual(self.script.count(safe), expected_count)
            script_without_safe_expansions = script_without_safe_expansions.replace(
                safe, ""
            )
            self.assertNotIn(unsafe, script_without_safe_expansions)

        result = subprocess.run(
            [
                "/bin/bash",
                "-uc",
                """
REMAINING_ARGS=()
set -- "${REMAINING_ARGS[@]+"${REMAINING_ARGS[@]}"}"
[ "$#" -eq 0 ]
REMAINING_ARGS=("alpha beta" "--verbose")
set -- "${REMAINING_ARGS[@]+"${REMAINING_ARGS[@]}"}"
[ "$#" -eq 2 ]
[ "$1" = "alpha beta" ]
[ "$2" = "--verbose" ]
""",
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_force_never_weakens_clean_tree_provenance(self):
        snapshot = self.script[
            self.script.index("snapshot_tests()") : self.script.index("# Parse arguments")
        ]
        self.assertIn('"status", "--porcelain", "--untracked-files=all"', self.provenance)
        self.assertIn('"dirty": False', self.provenance)
        self.assertNotIn('if [ "$FORCE_SNAPSHOT" != "true" ]; then\n        local dirty', snapshot)

    def test_snapshot_records_hashes_selection_and_terminal_identities(self):
        for token in (
            '"sha256"',
            '"full_domain": True',
            '"runner_args": args.runner_arg',
            '"corpus"',
            '"oracle_cache"',
            "terminal_failures",
            "return \"$runner_status\"",
        ):
            with self.subTest(token=token):
                self.assertIn(token, self.script + self.provenance)

    def test_artifact_extractors_are_fatal_and_atomic(self):
        snapshot = self.script[
            self.script.index("snapshot_tests()") : self.script.index("# Parse arguments")
        ]
        self.assertNotIn("|| true", snapshot)
        self.assertIn('mv "$detail_tmp" "$detail_file"', snapshot)
        self.assertIn('mv "$snapshot_tmp" "$snapshot_file"', snapshot)
        self.assertIn('mv "$baseline_tmp" "$baseline_file"', snapshot)


if __name__ == "__main__":
    unittest.main()
