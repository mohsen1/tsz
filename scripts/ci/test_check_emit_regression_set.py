"""Contract tests for the emit failing-row direction check (#16171).

The point of the gate under test is that it catches what the emit *count*
comparison structurally cannot: a swap (one row fixed, one broken, counts
unchanged) and a ratchet-down (a refreshed snapshot lowering the count bar).
Each of those has its own test below, exercised end to end rather than read off
the source.
"""

import io
import json
import pathlib
import subprocess
import tempfile
import unittest
import contextlib


ROOT = pathlib.Path(__file__).resolve().parents[2]
CHECKER = ROOT / "scripts" / "ci" / "check-emit-regression-set.py"
FULL_CI = ROOT / "scripts" / "ci" / "full-ci.sh"
COMMITTED_DETAIL = ROOT / "scripts" / "emit" / "emit-detail.json"

def _load_checker():
    """Import the hyphenated script as a module."""
    import importlib.util

    spec = importlib.util.spec_from_file_location("check_emit_regression_set", CHECKER)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


CHECK = _load_checker()


def row(name, js="pass", dts="skip"):
    return {
        "name": name,
        "baselineFile": "%s.js" % name,
        "testPath": "tests/cases/compiler/%s.ts" % name,
        "jsStatus": js,
        "dtsStatus": dts,
    }


def detail_doc(rows):
    return {"results": rows}


def write_json(path, payload):
    path.write_text(json.dumps(payload), encoding="utf-8")
    return path


def run_checker(baseline_rows, detail_row_sets):
    """Run the checker in-process; return (exit_code, stderr_text)."""
    with tempfile.TemporaryDirectory() as temp_dir:
        temp = pathlib.Path(temp_dir)
        baseline = write_json(temp / "baseline.json", detail_doc(baseline_rows))
        details = []
        for index, rows in enumerate(detail_row_sets):
            details.append(str(write_json(temp / ("detail-%d.json" % index), detail_doc(rows))))
        err = io.StringIO()
        out = io.StringIO()
        with contextlib.redirect_stderr(err), contextlib.redirect_stdout(out):
            code = CHECK.main(["--baseline", str(baseline)] + details)
        return code, err.getvalue(), out.getvalue()


class EmitRegressionSetTests(unittest.TestCase):
    def test_identical_sets_pass(self):
        rows = [row("a"), row("b", js="fail")]
        code, err, out = run_checker(rows, [rows])
        self.assertEqual(code, 0, err)
        self.assertIn("Emit regression set OK", out)

    def test_newly_failing_row_is_fatal_and_named(self):
        baseline = [row("a"), row("b")]
        current = [row("a"), row("b", js="fail")]
        code, err, _ = run_checker(baseline, [current])
        self.assertEqual(code, 1)
        self.assertIn("emit regression", err)
        self.assertIn("b", err)
        self.assertIn("pass -> fail", err)

    def test_swap_with_identical_counts_is_caught(self):
        """The hole the count gate cannot see: one row fixed, one broken."""
        baseline = [row("a", js="fail"), row("b"), row("c")]
        current = [row("a"), row("b", js="fail"), row("c")]

        baseline_pass = sum(1 for r in baseline if r["jsStatus"] == "pass")
        current_pass = sum(1 for r in current if r["jsStatus"] == "pass")
        self.assertEqual(baseline_pass, current_pass, "the swap must be count-neutral")

        code, err, _ = run_checker(baseline, [current])
        self.assertEqual(code, 1)
        self.assertIn("b", err)

    def test_ratcheted_down_baseline_still_catches_the_named_row(self):
        """A regressed row stays fatal even when the counts were refreshed with it.

        This models the #16171 trap: a hand-refreshed snapshot lowers the count
        bar, so the count comparison is satisfied. The set check is keyed on
        row identity, not totals, so it is unaffected.
        """
        baseline = [row("a"), row("b"), row("c")]
        current = [row("a"), row("b", js="fail"), row("c", js="fail")]
        code, err, _ = run_checker(baseline, [current])
        self.assertEqual(code, 1)
        self.assertIn("2 row(s) fail now", err)

    def test_dts_regression_is_caught_independently_of_js(self):
        baseline = [row("a", js="pass", dts="pass")]
        current = [row("a", js="pass", dts="fail")]
        code, err, _ = run_checker(baseline, [current])
        self.assertEqual(code, 1)
        self.assertIn("DTS", err)

    def test_timeout_counts_as_failing(self):
        baseline = [row("a")]
        current = [row("a", js="timeout")]
        code, err, _ = run_checker(baseline, [current])
        self.assertEqual(code, 1)
        self.assertIn("pass -> timeout", err)

    def test_fixing_a_baseline_failure_is_not_a_regression(self):
        baseline = [row("a", js="fail"), row("b", dts="fail")]
        current = [row("a"), row("b", dts="pass")]
        code, err, out = run_checker(baseline, [current])
        self.assertEqual(code, 0, err)
        self.assertIn("failing rows 2 -> 0", out)

    def test_new_corpus_row_absent_from_baseline_is_not_a_regression(self):
        baseline = [row("a")]
        current = [row("a"), row("brand_new", js="fail")]
        code, err, _ = run_checker(baseline, [current])
        self.assertEqual(code, 0, err)

    def test_absent_baseline_row_warns_but_does_not_fail(self):
        baseline = [row("a"), row("removed_by_submodule_bump")]
        current = [row("a")]
        code, err, _ = run_checker(baseline, [current])
        self.assertEqual(code, 0, err)
        self.assertIn("warning", err)
        self.assertIn("removed_by_submodule_bump", err)

    def test_shards_are_unioned_before_comparing(self):
        baseline = [row("a"), row("b"), row("c")]
        code, err, _ = run_checker(baseline, [[row("a")], [row("b")], [row("c")]])
        self.assertEqual(code, 0, err)
        self.assertNotIn("warning", err)

    def test_regression_in_any_shard_is_caught(self):
        baseline = [row("a"), row("b"), row("c")]
        code, err, _ = run_checker(baseline, [[row("a")], [row("b", js="fail")], [row("c")]])
        self.assertEqual(code, 1)
        self.assertIn("b", err)

    def test_empty_result_set_is_fatal(self):
        code, err, _ = run_checker([row("a")], [[]])
        self.assertEqual(code, 1)
        self.assertIn("no result rows", err)

    def test_missing_baseline_file_is_fatal(self):
        err = io.StringIO()
        with tempfile.TemporaryDirectory() as temp_dir:
            temp = pathlib.Path(temp_dir)
            detail = write_json(temp / "detail.json", detail_doc([row("a")]))
            with contextlib.redirect_stderr(err):
                code = CHECK.main(
                    ["--baseline", str(temp / "nope.json"), str(detail)]
                )
        self.assertEqual(code, 1)
        self.assertIn("needs a baseline", err.getvalue())

    def test_row_key_is_unique_across_the_committed_baseline(self):
        """The whole check rests on (testPath, baselineFile, name) being an identity."""
        data = json.loads(COMMITTED_DETAIL.read_text(encoding="utf-8"))
        results = data["results"]
        keys = [CHECK.row_key(r) for r in results]
        self.assertEqual(len(keys), len(set(keys)))

    def test_committed_baseline_compares_clean_against_itself(self):
        """Self-comparison must be a no-op, or the gate would fail on a green run."""
        with tempfile.TemporaryDirectory() as temp_dir:
            err, out = io.StringIO(), io.StringIO()
            with contextlib.redirect_stderr(err), contextlib.redirect_stdout(out):
                code = CHECK.main(
                    ["--baseline", str(COMMITTED_DETAIL), str(COMMITTED_DETAIL)]
                )
            self.assertEqual(code, 0, err.getvalue())
            self.assertIn("no row newly failing", out.getvalue())


class EmitRegressionSetWiringTests(unittest.TestCase):
    """The gate is only real if the emit leaves actually call it."""

    @classmethod
    def setUpClass(cls):
        cls.script = FULL_CI.read_text(encoding="utf-8")

    def function_body(self, name, end_marker):
        start = self.script.index("%s() {" % name)
        end = self.script.index(end_marker, start)
        return self.script[start:end]

    def test_single_shard_path_runs_the_set_check_before_publishing(self):
        body = self.function_body("run_emit_shard", "\n# Recombine the per-shard")
        counts_idx = body.index("validate_emit_aggregate_counts")
        set_idx = body.index("validate_emit_regression_set", counts_idx)
        publish_idx = body.index("publish_latest_metric emit", set_idx)
        self.assertLess(counts_idx, set_idx)
        self.assertLess(set_idx, publish_idx)

    def test_aggregate_path_runs_the_set_check_before_publishing(self):
        body = self.function_body("run_emit_aggregate", "\nrun_fourslash_shard() {")
        counts_idx = body.index("validate_emit_aggregate_counts")
        set_idx = body.index("validate_emit_regression_set", counts_idx)
        publish_idx = body.index("publish_latest_metric emit", set_idx)
        self.assertLess(counts_idx, set_idx)
        self.assertLess(set_idx, publish_idx)

    def test_aggregate_collects_per_shard_detail_from_artifacts(self):
        body = self.function_body("run_emit_aggregate", "\nrun_fourslash_shard() {")
        self.assertIn('-name "emit-detail-*.json"', body)
        self.assertIn('cp "$detail" "$tmp_dir/detail-', body)

    def test_workflow_uploads_the_detail_the_aggregate_reads(self):
        workflow = (ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
        self.assertIn("emit-detail-${{ matrix.shard }}.json", workflow)

    def test_set_check_fails_closed_with_no_detail(self):
        helper = self.function_body("validate_emit_regression_set", "\nrun_emit_shard() {")
        with tempfile.TemporaryDirectory() as temp_dir:
            runner = pathlib.Path(temp_dir) / "run.sh"
            runner.write_text(
                "#!/usr/bin/env bash\nset -Eeuo pipefail\n%s\nvalidate_emit_regression_set\n" % helper,
                encoding="utf-8",
            )
            proc = subprocess.run(
                ["bash", str(runner)], cwd=str(ROOT), capture_output=True, text=True
            )
        self.assertNotEqual(proc.returncode, 0)
        self.assertIn("no per-test detail JSON", proc.stderr)

    def test_set_check_invokes_the_checker_against_the_committed_baseline(self):
        helper = self.function_body("validate_emit_regression_set", "\nrun_emit_shard() {")
        with tempfile.TemporaryDirectory() as temp_dir:
            temp = pathlib.Path(temp_dir)
            detail = write_json(temp / "detail.json", detail_doc([row("a")]))
            runner = temp / "run.sh"
            runner.write_text(
                "#!/usr/bin/env bash\nset -Eeuo pipefail\n%s\nvalidate_emit_regression_set '%s'\n"
                % (helper, detail),
                encoding="utf-8",
            )
            proc = subprocess.run(
                ["bash", str(runner)], cwd=str(ROOT), capture_output=True, text=True
            )
        # The row is unknown to the real baseline, so this is a clean run that
        # proves the wiring reaches scripts/emit/emit-detail.json.
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("Emit regression set OK", proc.stdout)


if __name__ == "__main__":
    unittest.main()
