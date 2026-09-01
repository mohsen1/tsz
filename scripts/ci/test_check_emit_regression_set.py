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
COMMITTED_DETAIL = ROOT / "scripts" / "emit" / "rewrite-regression-baseline.json"

def _load_checker():
    """Import the hyphenated script as a module."""
    import importlib.util

    spec = importlib.util.spec_from_file_location("check_emit_regression_set", CHECKER)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


CHECK = _load_checker()


def row(
    name,
    js="pass",
    dts="skip",
    js_error=None,
    dts_error=None,
    artifact=None,
):
    if artifact is None:
        artifact = "complete" if js in ("pass", "fail", "mystery") else js
    result = {
        "name": name,
        "baselineFile": "%s.js" % name,
        "testPath": "tests/cases/compiler/%s.ts" % name,
        "jsStatus": js,
        "dtsStatus": dts,
        "artifactState": artifact,
    }
    if js_error is not None:
        result["jsError"] = js_error
    if dts_error is not None:
        result["dtsError"] = dts_error
    return result


def v2_row(
    name,
    *,
    artifact="complete",
    outcome_match=True,
    js_selected=True,
    dts_selected=False,
    js_product_match=True,
    dts_product_match=None,
):
    if dts_selected and dts_product_match is None:
        dts_product_match = True
    if not js_selected:
        js_product_match = None
    if not dts_selected:
        dts_product_match = None

    result = {
        "name": name,
        "baselineFile": "%s.js" % name,
        "testPath": "tests/cases/compiler/%s.ts" % name,
        "artifactState": artifact,
        "outcomeMatch": outcome_match,
        "jsSelected": js_selected,
        "dtsSelected": dts_selected,
    }
    for surface, selected, product_match in (
        ("js", js_selected, js_product_match),
        ("dts", dts_selected, dts_product_match),
    ):
        match = outcome_match is True and product_match is True if selected else None
        status = (
            ("pass" if match else "fail") if artifact == "complete" else artifact
        ) if selected else "skip"
        result[f"{surface}Match"] = match
        result[f"{surface}ProductMatch"] = product_match
        result[f"{surface}Status"] = status
        if product_match is False:
            product_error = "Content mismatch at out.js: +1/-1 lines"
            result[f"{surface}ProductError"] = product_error
        else:
            product_error = None
        if selected and not match:
            result[f"{surface}Error"] = (
                "TSZ_NONZERO_OUTCOME: exit=3, diagnostics=<none>"
                if outcome_match is False
                else product_error or "typed terminal state"
            )
    if outcome_match is False:
        result["outcomeError"] = "TSZ_NONZERO_OUTCOME: exit=3, diagnostics=<none>"
    return result


def detail_doc(rows, oracle=None):
    result = {
        "schemaVersion": 1,
        "sourceArtifactSha256": "sha256:" + ("0" * 64),
        "git_sha": "0" * 40,
        "detailResultCount": len(rows),
        "results": rows,
    }
    if oracle is not None:
        result["oracle"] = {"fingerprint": oracle}
    return result


def write_json(path, payload):
    path.write_text(json.dumps(payload), encoding="utf-8")
    return path


def git(root, *arguments):
    return subprocess.run(
        ["git", "-C", str(root), *arguments],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()


def run_checker(
    baseline_rows,
    detail_row_sets,
    baseline_oracle=None,
    detail_oracles=None,
    require_oracle=False,
    reject_absent=False,
):
    """Run the checker in-process; return (exit_code, stderr_text)."""
    with tempfile.TemporaryDirectory() as temp_dir:
        temp = pathlib.Path(temp_dir)
        baseline = write_json(
            temp / "baseline.json", detail_doc(baseline_rows, baseline_oracle)
        )
        details = []
        for index, rows in enumerate(detail_row_sets):
            oracle = detail_oracles[index] if detail_oracles is not None else None
            details.append(
                str(
                    write_json(
                        temp / ("detail-%d.json" % index),
                        detail_doc(rows, oracle),
                    )
                )
            )
        err = io.StringIO()
        out = io.StringIO()
        with contextlib.redirect_stderr(err), contextlib.redirect_stdout(out):
            args = ["--baseline", str(baseline)]
            if require_oracle:
                args.extend(
                    [
                        "--require-oracle-provenance",
                        "--oracle-manifest",
                        str(ROOT / "scripts/emit/oracle-manifest.json"),
                    ]
                )
            if reject_absent:
                args.append("--reject-absent-baseline-rows")
            code = CHECK.main(args + details)
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
        self.assertIn("2 named product status transition(s)", err)

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

    def test_pass_to_incomplete_is_a_named_pass_loss(self):
        baseline = [row("a", js="pass")]
        current = [row("a", js="incomplete")]
        code, err, _ = run_checker(baseline, [current])
        self.assertEqual(code, 1)
        self.assertIn("pass -> incomplete", err)

    def test_incomplete_to_crash_is_a_terminal_escalation(self):
        baseline = [row("a", js="incomplete")]
        current = [row("a", js="crash")]
        code, err, _ = run_checker(baseline, [current])
        self.assertEqual(code, 1)
        self.assertIn("incomplete -> crash", err)

    def test_fail_to_crash_is_a_terminal_escalation(self):
        baseline = [row("a", js="fail")]
        current = [row("a", js="crash")]
        code, err, _ = run_checker(baseline, [current])
        self.assertEqual(code, 1)
        self.assertIn("fail -> crash", err)

    def test_terminal_row_may_become_typed_unsupported(self):
        baseline = [row("a", js="crash", dts="crash")]
        current = [row("a", js="unsupported", dts="unsupported")]
        code, err, _ = run_checker(baseline, [current])
        self.assertEqual(code, 0, err)

    def test_complete_artifact_to_incomplete_is_a_named_regression(self):
        baseline = [row("a", artifact="complete")]
        current = [row("a", js="incomplete", artifact="incomplete")]
        code, err, _ = run_checker(baseline, [current])
        self.assertEqual(code, 1)
        self.assertIn("Artifact", err)
        self.assertIn("complete -> incomplete", err)

    def test_incomplete_artifact_cannot_withdraw_to_unsupported(self):
        baseline = [row("a", js="incomplete", artifact="incomplete")]
        current = [row("a", js="unsupported", dts="unsupported")]
        code, err, _ = run_checker(baseline, [current])
        self.assertEqual(code, 1)
        self.assertIn("Artifact", err)
        self.assertIn("incomplete -> unsupported", err)

    def test_measured_product_cannot_withdraw_to_unsupported(self):
        baseline = [row("a", js="incomplete", artifact="incomplete")]
        current = [row("a", js="unsupported", dts="unsupported")]
        code, err, _ = run_checker(baseline, [current])
        self.assertEqual(code, 1)
        self.assertIn("JS", err)
        self.assertIn("incomplete -> unsupported", err)

    def test_selected_dts_product_cannot_become_skip(self):
        baseline = [row("a", js="pass", dts="pass")]
        current = [row("a", js="pass", dts="skip")]
        code, err, _ = run_checker(baseline, [current])
        self.assertEqual(code, 1)
        self.assertIn("DTS", err)
        self.assertIn("pass -> skip", err)

    def test_missing_required_status_field_is_fatal(self):
        current = row("a")
        del current["artifactState"]
        code, err, _ = run_checker([row("a")], [[current]])
        self.assertEqual(code, 1)
        self.assertIn("omits required Artifact status field", err)

    def test_cross_field_status_inconsistency_is_fatal(self):
        current = row("a", js="incomplete", dts="skip")
        current["artifactState"] = "complete"
        code, err, _ = run_checker([row("a")], [[current]])
        self.assertEqual(code, 1)
        self.assertIn("inconsistent js artifact product status", err)

    def test_schema_v2_dts_only_row_keeps_unselected_js_product_neutral(self):
        rows = [v2_row("a", js_selected=False, dts_selected=True)]

        code, err, out = run_checker(rows, [rows])

        self.assertEqual(code, 0, err)
        self.assertIn("Emit regression set OK", out)
        self.assertEqual(rows[0]["jsStatus"], "skip")
        self.assertIsNone(rows[0]["jsMatch"])
        self.assertIsNone(rows[0]["jsProductMatch"])

    def test_schema_v2_js_only_row_keeps_unselected_dts_product_neutral(self):
        rows = [v2_row("a", js_selected=True, dts_selected=False)]

        code, err, out = run_checker(rows, [rows])

        self.assertEqual(code, 0, err)
        self.assertIn("Emit regression set OK", out)
        self.assertEqual(rows[0]["dtsStatus"], "skip")
        self.assertIsNone(rows[0]["dtsMatch"])
        self.assertIsNone(rows[0]["dtsProductMatch"])

    def test_schema_v2_unselected_js_product_must_be_skip(self):
        current = v2_row("a", js_selected=False, dts_selected=True)
        current["jsStatus"] = "incomplete"

        code, err, _ = run_checker(
            [v2_row("a", js_selected=False, dts_selected=True)],
            [[current]],
        )

        self.assertEqual(code, 1)
        self.assertIn("selected=False", err)
        self.assertIn("status=incomplete", err)

    def test_schema_v2_unselected_product_cannot_claim_raw_parity(self):
        current = v2_row("a", js_selected=False, dts_selected=True)
        current["jsProductMatch"] = False

        code, err, _ = run_checker(
            [v2_row("a", js_selected=False, dts_selected=True)],
            [[current]],
        )

        self.assertEqual(code, 1)
        self.assertIn("unselected JS product with measured parity", err)

    def test_schema_v2_terminal_product_mismatch_keeps_both_facts(self):
        rows = [
            v2_row(
                "a",
                artifact="incomplete",
                outcome_match=False,
                js_product_match=False,
            )
        ]

        code, err, out = run_checker(rows, [rows])

        self.assertEqual(code, 0, err)
        self.assertIn("Emit regression set OK", out)
        self.assertEqual(rows[0]["jsStatus"], "incomplete")
        self.assertFalse(rows[0]["jsProductMatch"])
        self.assertIn("Content mismatch", rows[0]["jsProductError"])

    def test_schema_v2_product_mismatch_requires_product_error(self):
        current = v2_row(
            "a",
            artifact="incomplete",
            outcome_match=False,
            js_product_match=False,
        )
        del current["jsProductError"]

        code, err, _ = run_checker(
            [
                v2_row(
                    "a",
                    artifact="incomplete",
                    outcome_match=False,
                    js_product_match=False,
                )
            ],
            [[current]],
        )

        self.assertEqual(code, 1)
        self.assertIn("jsProductMatch=false without jsProductError", err)

    def test_schema_v2_terminal_row_cannot_lose_proven_product_parity(self):
        baseline = [
            v2_row(
                "a",
                artifact="incomplete",
                outcome_match=False,
                js_product_match=True,
            )
        ]
        current = [
            v2_row(
                "a",
                artifact="incomplete",
                outcome_match=False,
                js_product_match=False,
            )
        ]

        code, err, _ = run_checker(baseline, [current])

        self.assertEqual(code, 1)
        self.assertIn("emit product parity regression", err)
        self.assertIn("productMatch=True -> False", err)

    def test_schema_v2_terminal_product_mismatch_may_improve(self):
        baseline = [
            v2_row(
                "a",
                artifact="incomplete",
                outcome_match=False,
                js_product_match=False,
            )
        ]
        current = [
            v2_row(
                "a",
                artifact="incomplete",
                outcome_match=False,
                js_product_match=True,
            )
        ]

        code, err, out = run_checker(baseline, [current])

        self.assertEqual(code, 0, err)
        self.assertIn("Emit regression set OK", out)

    def test_passing_product_cannot_retain_an_error_payload(self):
        current = row(
            "a",
            js="pass",
            js_error="TSZ_NONZERO_OUTCOME: exit=3, diagnostics=TS2304",
        )
        code, err, _ = run_checker([row("a")], [[current]])
        self.assertEqual(code, 1)
        self.assertIn("jsStatus=pass with a jsError payload", err)

    def test_skipped_product_cannot_retain_an_error_payload(self):
        current = row("a", dts="skip", dts_error="stale DTS failure")
        code, err, _ = run_checker([row("a")], [[current]])
        self.assertEqual(code, 1)
        self.assertIn("dtsStatus=skip with a dtsError payload", err)

    def test_unknown_product_status_is_fatal(self):
        current = [row("a", js="mystery")]
        code, err, _ = run_checker([row("a")], [current])
        self.assertEqual(code, 1)
        self.assertIn("unknown JS status", err)

    def test_fixing_a_baseline_failure_is_not_a_regression(self):
        baseline = [row("a", js="fail"), row("b", dts="fail")]
        current = [row("a"), row("b", dts="pass")]
        code, err, out = run_checker(baseline, [current])
        self.assertEqual(code, 0, err)
        self.assertIn("failing rows 2 -> 0", out)

    def test_same_status_added_oracle_clean_diagnostic_is_fatal(self):
        baseline = [
            row(
                "a",
                js="incomplete",
                js_error="TSZ_NONZERO_OUTCOME: exit=3, diagnostics=TS1124",
            )
        ]
        current = [
            row(
                "a",
                js="incomplete",
                js_error=(
                    "TSZ_NONZERO_OUTCOME: exit=3, "
                    "diagnostics=TS1124,TS2304"
                ),
            )
        ]
        code, err, _ = run_checker(baseline, [current])
        self.assertEqual(code, 1)
        self.assertIn("diagnostic regression", err)
        self.assertIn("TS2304", err)
        self.assertIn("a", err)

    def test_oracle_clean_diagnostic_removal_is_an_improvement(self):
        baseline = [
            row(
                "a",
                js="incomplete",
                js_error=(
                    "TSZ_NONZERO_OUTCOME: exit=3, "
                    "diagnostics=TS1124,TS2304"
                ),
            )
        ]
        current = [
            row(
                "a",
                js="incomplete",
                js_error="TSZ_NONZERO_OUTCOME: exit=3, diagnostics=TS1124",
            )
        ]
        code, err, out = run_checker(baseline, [current])
        self.assertEqual(code, 0, err)
        self.assertIn("no oracle-clean TSZ diagnostic grew", out)

    def test_oracle_clean_diagnostic_reorder_is_fatal(self):
        baseline = [
            row(
                "a",
                js="incomplete",
                js_error=(
                    "TSZ_NONZERO_OUTCOME: exit=3, "
                    "diagnostics=TS1124,TS1109"
                ),
            )
        ]
        current = [
            row(
                "a",
                js="incomplete",
                js_error=(
                    "TSZ_NONZERO_OUTCOME: exit=3, "
                    "diagnostics=TS1109,TS1124"
                ),
            )
        ]
        code, err, _ = run_checker(baseline, [current])
        self.assertEqual(code, 1)
        self.assertIn("reordered", err)

    def test_typed_nonclaim_without_diagnostics_is_not_payload_regression(self):
        baseline = [
            row(
                "a",
                js="incomplete",
                js_error="TSZ_NONZERO_OUTCOME: exit=3, diagnostics=<none>",
            )
        ]
        current = [
            row(
                "a",
                js="incomplete",
                js_error="TSZ_NONZERO_OUTCOME: exit=3, diagnostics=<none>",
            )
        ]
        code, err, _ = run_checker(baseline, [current])
        self.assertEqual(code, 0, err)

    def test_new_oracle_clean_dts_diagnostic_is_fatal(self):
        baseline = [row("a", js="incomplete", dts="skip")]
        current = [
            row(
                "a",
                js="incomplete",
                dts="incomplete",
                dts_error="TSZ_NONZERO_OUTCOME: exit=3, diagnostics=TS2304",
            )
        ]
        code, err, _ = run_checker(baseline, [current])
        self.assertEqual(code, 1)
        self.assertIn("DTS", err)
        self.assertIn("TS2304", err)

    def test_new_corpus_row_absent_from_baseline_is_not_a_regression(self):
        baseline = [row("a")]
        current = [row("a"), row("brand_new", js="fail")]
        code, err, _ = run_checker(baseline, [current])
        self.assertEqual(code, 0, err)

    def test_new_corpus_oracle_clean_diagnostic_is_fatal(self):
        baseline = [row("a")]
        current = [
            row("a"),
            row(
                "brand_new",
                js="incomplete",
                js_error="TSZ_NONZERO_OUTCOME: exit=3, diagnostics=TS2304",
            ),
        ]
        code, err, _ = run_checker(baseline, [current])
        self.assertEqual(code, 1)
        self.assertIn("brand_new", err)
        self.assertIn("TS2304", err)

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

    def test_duplicate_key_within_one_detail_is_fatal(self):
        code, err, _ = run_checker([row("a")], [[row("a"), row("a")]])
        self.assertEqual(code, 1)
        self.assertIn("duplicate emit row", err)

    def test_incomplete_stable_key_is_fatal(self):
        malformed = row("a")
        malformed["testPath"] = ""
        code, err, _ = run_checker([row("a")], [[malformed]])
        self.assertEqual(code, 1)
        self.assertIn("incomplete stable key", err)

    def test_declared_result_count_must_match_rows(self):
        err = io.StringIO()
        with tempfile.TemporaryDirectory() as temp_dir:
            temp = pathlib.Path(temp_dir)
            baseline = write_json(temp / "baseline.json", detail_doc([row("a")]))
            current = detail_doc([row("a")])
            current["detailResultCount"] = 2
            detail = write_json(temp / "detail.json", current)
            with contextlib.redirect_stderr(err):
                code = CHECK.main(
                    ["--baseline", str(baseline), str(detail)]
                )
        self.assertEqual(code, 1)
        self.assertIn("detailResultCount=2", err.getvalue())

    def test_detail_result_count_is_required(self):
        err = io.StringIO()
        with tempfile.TemporaryDirectory() as temp_dir:
            temp = pathlib.Path(temp_dir)
            baseline = write_json(temp / "baseline.json", detail_doc([row("a")]))
            current = write_json(temp / "detail.json", {"results": [row("a")]})
            with contextlib.redirect_stderr(err):
                code = CHECK.main(["--baseline", str(baseline), str(current)])
        self.assertEqual(code, 1)
        self.assertIn("omits required detailResultCount", err.getvalue())

    def test_typed_outcome_cannot_disappear_while_row_remains_incomplete(self):
        baseline = [
            row(
                "a",
                js="incomplete",
                js_error="TSZ_NONZERO_OUTCOME: exit=3, diagnostics=TS2304",
            )
        ]
        current = [row("a", js="incomplete")]
        code, err, _ = run_checker(baseline, [current])
        self.assertEqual(code, 1)
        self.assertIn("erased typed oracle-clean outcome", err)

    def test_typed_outcome_cannot_be_replaced_by_generic_error(self):
        baseline = [
            row(
                "a",
                js="incomplete",
                js_error="TSZ_NONZERO_OUTCOME: exit=3, diagnostics=TS2304",
            )
        ]
        current = [row("a", js="incomplete", js_error="generic runner error")]
        code, err, _ = run_checker(baseline, [current])
        self.assertEqual(code, 1)
        self.assertIn("missing-or-unrecognized-outcome", err)

    def test_typed_outcome_may_become_explicit_empty_nonclaim(self):
        baseline = [
            row(
                "a",
                js="incomplete",
                js_error="TSZ_NONZERO_OUTCOME: exit=3, diagnostics=TS2304",
            )
        ]
        current = [
            row(
                "a",
                js="incomplete",
                js_error="TSZ_NONZERO_OUTCOME: exit=3, diagnostics=<none>",
            )
        ]
        code, err, _ = run_checker(baseline, [current])
        self.assertEqual(code, 0, err)

    def test_passing_row_may_drop_old_typed_outcome(self):
        baseline = [
            row(
                "a",
                js="incomplete",
                js_error="TSZ_NONZERO_OUTCOME: exit=3, diagnostics=TS2304",
            )
        ]
        current = [row("a")]
        code, err, _ = run_checker(baseline, [current])
        self.assertEqual(code, 0, err)

    def test_duplicate_key_across_shards_is_fatal(self):
        code, err, _ = run_checker(
            [row("a")], [[row("a")], [row("a")]]
        )
        self.assertEqual(code, 1)
        self.assertIn("already reported by another detail", err)

    def test_untrusted_oracle_fingerprint_is_fatal(self):
        trusted = sorted(
            CHECK.trusted_oracle_fingerprints(
                ROOT / "scripts/emit/oracle-manifest.json"
            )
        )[0]
        code, err, _ = run_checker(
            [row("a")],
            [[row("a")]],
            baseline_oracle=trusted,
            detail_oracles=["sha256:two"],
            require_oracle=True,
        )
        self.assertEqual(code, 1)
        self.assertIn("untrusted oracle fingerprint", err)

    def test_oracle_fingerprint_agreement_passes(self):
        trusted = sorted(
            CHECK.trusted_oracle_fingerprints(
                ROOT / "scripts/emit/oracle-manifest.json"
            )
        )[0]
        code, err, _ = run_checker(
            [row("a"), row("b")],
            [[row("a")], [row("b")]],
            baseline_oracle=trusted,
            detail_oracles=[trusted, trusted],
            require_oracle=True,
        )
        self.assertEqual(code, 0, err)

    def test_required_oracle_provenance_cannot_be_omitted(self):
        trusted = sorted(
            CHECK.trusted_oracle_fingerprints(
                ROOT / "scripts/emit/oracle-manifest.json"
            )
        )[0]
        code, err, _ = run_checker(
            [row("a")],
            [[row("a")]],
            baseline_oracle=trusted,
            require_oracle=True,
        )
        self.assertEqual(code, 1)
        self.assertIn("omit required oracle provenance", err)

    def test_required_baseline_oracle_provenance_cannot_be_omitted(self):
        trusted = sorted(
            CHECK.trusted_oracle_fingerprints(
                ROOT / "scripts/emit/oracle-manifest.json"
            )
        )[0]
        code, err, _ = run_checker(
            [row("a")],
            [[row("a")]],
            detail_oracles=[trusted],
            require_oracle=True,
        )
        self.assertEqual(code, 1)
        self.assertIn("baseline omits required oracle provenance", err)

    def test_required_baseline_projection_provenance_cannot_be_omitted(self):
        trusted = sorted(
            CHECK.trusted_oracle_fingerprints(
                ROOT / "scripts/emit/oracle-manifest.json"
            )
        )[0]
        with tempfile.TemporaryDirectory() as temp_dir:
            temp = pathlib.Path(temp_dir)
            baseline_doc = detail_doc([row("a")], trusted)
            del baseline_doc["sourceArtifactSha256"]
            baseline = write_json(temp / "baseline.json", baseline_doc)
            detail = write_json(temp / "detail.json", detail_doc([row("a")], trusted))
            err = io.StringIO()
            with contextlib.redirect_stderr(err):
                code = CHECK.main(
                    [
                        "--baseline",
                        str(baseline),
                        "--require-oracle-provenance",
                        "--oracle-manifest",
                        str(ROOT / "scripts/emit/oracle-manifest.json"),
                        str(detail),
                    ]
                )
        self.assertEqual(code, 1)
        self.assertIn("invalid sourceArtifactSha256 provenance", err.getvalue())

    def test_malformed_tsz_nonzero_outcome_is_fatal(self):
        current = [
            row(
                "a",
                js="incomplete",
                js_error="TSZ_NONZERO_OUTCOME: exit=3",
            )
        ]
        code, err, _ = run_checker([row("a")], [current])
        self.assertEqual(code, 1)
        self.assertIn("malformed TSZ_NONZERO_OUTCOME", err)

    def test_regression_in_any_shard_is_caught(self):
        baseline = [row("a"), row("b"), row("c")]
        code, err, _ = run_checker(baseline, [[row("a")], [row("b", js="fail")], [row("c")]])
        self.assertEqual(code, 1)
        self.assertIn("b", err)

    def test_missing_baseline_row_is_fatal_when_ci_requires_complete_details(self):
        code, err, _ = run_checker(
            [row("a"), row("b")], [[row("a")]], reject_absent=True
        )
        self.assertEqual(code, 1)
        self.assertIn("error: 1 baseline emit row", err)
        self.assertIn("absent b", err)

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
        self.assertEqual(data["detailResultCount"], 13806)
        self.assertEqual(len(results), 13806)
        self.assertEqual(
            data["sourceArtifactSha256"],
            "sha256:21a8bbe10dc00effc0828cf484b33d15f8816a651352694a70b6c3e46aa58a97",
        )
        self.assertEqual(len(keys), len(set(keys)))

    def test_committed_baseline_compares_clean_against_itself(self):
        """Self-comparison must be a no-op, or the gate would fail on a green run."""
        with tempfile.TemporaryDirectory() as temp_dir:
            err, out = io.StringIO(), io.StringIO()
            with contextlib.redirect_stderr(err), contextlib.redirect_stdout(out):
                code = CHECK.main(
                    [
                        "--baseline",
                        str(COMMITTED_DETAIL),
                        "--require-oracle-provenance",
                        "--reject-absent-baseline-rows",
                        "--oracle-manifest",
                        str(ROOT / "scripts/emit/oracle-manifest.json"),
                        str(COMMITTED_DETAIL),
                    ]
                )
            self.assertEqual(code, 0, err.getvalue())
            self.assertIn("no row newly failing", out.getvalue())

    def test_baseline_history_blocks_later_same_branch_regression(self):
        trusted = sorted(
            CHECK.trusted_oracle_fingerprints(
                ROOT / "scripts/emit/oracle-manifest.json"
            )
        )[0]
        with tempfile.TemporaryDirectory() as temp_dir:
            repo = pathlib.Path(temp_dir)
            git(repo, "init", "--quiet")
            git(repo, "config", "user.email", "emit@example.invalid")
            git(repo, "config", "user.name", "Emit Guard")
            baseline = repo / "scripts/emit/rewrite-regression-baseline.json"
            baseline.parent.mkdir(parents=True)
            write_json(
                baseline,
                detail_doc(
                    [
                        row(
                            "a",
                            js="incomplete",
                            js_error=(
                                "TSZ_NONZERO_OUTCOME: exit=3, diagnostics=TS1124"
                            ),
                        )
                    ],
                    trusted,
                ),
            )
            git(repo, "add", ".")
            git(repo, "commit", "--quiet", "-m", "introduce baseline")
            write_json(
                baseline,
                detail_doc(
                    [
                        row(
                            "a",
                            js="incomplete",
                            js_error=(
                                "TSZ_NONZERO_OUTCOME: exit=3, "
                                "diagnostics=TS1124,TS2304"
                            ),
                        )
                    ],
                    trusted,
                ),
            )
            git(repo, "add", ".")
            git(repo, "commit", "--quiet", "-m", "try to lower floor")
            err = io.StringIO()
            with contextlib.redirect_stderr(err), contextlib.redirect_stdout(
                io.StringIO()
            ):
                code = CHECK.check_baseline_history(
                    repo,
                    baseline,
                    ROOT / "scripts/emit/oracle-manifest.json",
                    CHECKER,
                )
        self.assertEqual(code, 1)
        self.assertIn("TS2304", err.getvalue())

    def test_baseline_history_survives_delete_and_recreate(self):
        trusted = sorted(
            CHECK.trusted_oracle_fingerprints(
                ROOT / "scripts/emit/oracle-manifest.json"
            )
        )[0]
        with tempfile.TemporaryDirectory() as temp_dir:
            repo = pathlib.Path(temp_dir)
            git(repo, "init", "--quiet")
            git(repo, "config", "user.email", "emit@example.invalid")
            git(repo, "config", "user.name", "Emit Guard")
            baseline = repo / "scripts/emit/rewrite-regression-baseline.json"
            baseline.parent.mkdir(parents=True)
            write_json(baseline, detail_doc([row("a")], trusted))
            git(repo, "add", ".")
            git(repo, "commit", "--quiet", "-m", "introduce baseline")
            baseline.unlink()
            git(repo, "add", "-u")
            git(repo, "commit", "--quiet", "-m", "delete baseline")
            write_json(baseline, detail_doc([row("a", js="fail")], trusted))
            git(repo, "add", ".")
            git(repo, "commit", "--quiet", "-m", "restore lower floor")
            err = io.StringIO()
            with contextlib.redirect_stderr(err), contextlib.redirect_stdout(
                io.StringIO()
            ):
                code = CHECK.check_baseline_history(
                    repo,
                    baseline,
                    ROOT / "scripts/emit/oracle-manifest.json",
                    CHECKER,
                )
        self.assertEqual(code, 1)
        self.assertIn("pass -> fail", err.getvalue())

    def test_baseline_history_keeps_strict_parallel_branch_floor(self):
        trusted = sorted(
            CHECK.trusted_oracle_fingerprints(
                ROOT / "scripts/emit/oracle-manifest.json"
            )
        )[0]
        with tempfile.TemporaryDirectory() as temp_dir:
            repo = pathlib.Path(temp_dir)
            git(repo, "init", "--quiet")
            git(repo, "config", "user.email", "emit@example.invalid")
            git(repo, "config", "user.name", "Emit Guard")
            readme = repo / "README.md"
            readme.write_text("base\n", encoding="utf-8")
            git(repo, "add", ".")
            git(repo, "commit", "--quiet", "-m", "base")
            base = git(repo, "rev-parse", "HEAD")

            git(repo, "checkout", "--quiet", "-b", "strict", base)
            baseline = repo / "scripts/emit/rewrite-regression-baseline.json"
            baseline.parent.mkdir(parents=True)
            write_json(baseline, detail_doc([row("a")], trusted))
            git(repo, "add", ".")
            git(repo, "commit", "--quiet", "-m", "strict baseline")

            git(repo, "checkout", "--quiet", "-b", "loose", base)
            baseline.parent.mkdir(parents=True, exist_ok=True)
            write_json(baseline, detail_doc([row("a", js="fail")], trusted))
            git(repo, "add", ".")
            git(repo, "commit", "--quiet", "-m", "loose baseline")
            git(repo, "merge", "--quiet", "--no-ff", "-X", "ours", "strict", "-m", "merge")

            err = io.StringIO()
            with contextlib.redirect_stderr(err), contextlib.redirect_stdout(
                io.StringIO()
            ):
                code = CHECK.check_baseline_history(
                    repo,
                    baseline,
                    ROOT / "scripts/emit/oracle-manifest.json",
                    CHECKER,
                )
        self.assertEqual(code, 1)
        self.assertIn("pass -> fail", err.getvalue())

    def test_baseline_history_rejects_symlink_reset(self):
        trusted = sorted(
            CHECK.trusted_oracle_fingerprints(
                ROOT / "scripts/emit/oracle-manifest.json"
            )
        )[0]
        with tempfile.TemporaryDirectory() as temp_dir:
            repo = pathlib.Path(temp_dir)
            git(repo, "init", "--quiet")
            git(repo, "config", "user.email", "emit@example.invalid")
            git(repo, "config", "user.name", "Emit Guard")
            baseline = repo / "scripts/emit/rewrite-regression-baseline.json"
            baseline.parent.mkdir(parents=True)
            write_json(baseline, detail_doc([row("a")], trusted))
            git(repo, "add", ".")
            git(repo, "commit", "--quiet", "-m", "strict baseline")
            loose = baseline.parent / "loose.json"
            write_json(loose, detail_doc([row("a", js="fail")], trusted))
            baseline.unlink()
            baseline.symlink_to(loose.name)
            git(repo, "add", ".")
            git(repo, "commit", "--quiet", "-m", "replace baseline with symlink")
            with self.assertRaisesRegex(ValueError, "must not contain a symlink"):
                CHECK.check_baseline_history(
                    repo,
                    baseline,
                    ROOT / "scripts/emit/oracle-manifest.json",
                    CHECKER,
                )

    def test_baseline_history_rejects_relevant_shallow_clone(self):
        trusted = sorted(
            CHECK.trusted_oracle_fingerprints(
                ROOT / "scripts/emit/oracle-manifest.json"
            )
        )[0]
        with tempfile.TemporaryDirectory() as temp_dir:
            source = pathlib.Path(temp_dir) / "source"
            source.mkdir()
            git(source, "init", "--quiet")
            git(source, "config", "user.email", "emit@example.invalid")
            git(source, "config", "user.name", "Emit Guard")
            baseline = source / "scripts/emit/rewrite-regression-baseline.json"
            baseline.parent.mkdir(parents=True)
            write_json(baseline, detail_doc([row("a")], trusted))
            git(source, "add", ".")
            git(source, "commit", "--quiet", "-m", "baseline")
            marker = source / "marker.txt"
            marker.write_text("later\n", encoding="utf-8")
            git(source, "add", ".")
            git(source, "commit", "--quiet", "-m", "later")
            clone = pathlib.Path(temp_dir) / "shallow"
            subprocess.run(
                [
                    "git",
                    "clone",
                    "--quiet",
                    "--depth",
                    "1",
                    source.as_uri(),
                    str(clone),
                ],
                check=True,
            )
            with self.assertRaisesRegex(ValueError, "requires complete history"):
                CHECK.check_baseline_history(
                    clone,
                    clone / "scripts/emit/rewrite-regression-baseline.json",
                    ROOT / "scripts/emit/oracle-manifest.json",
                    CHECKER,
                )


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
        set_idx = body.index("validate_emit_regression_set")
        counts_idx = body.index("validate_emit_aggregate_counts", set_idx)
        publish_idx = body.index("publish_latest_metric emit", set_idx)
        self.assertLess(set_idx, counts_idx)
        self.assertLess(counts_idx, publish_idx)

    def test_aggregate_path_runs_the_set_check_before_publishing(self):
        body = self.function_body("run_emit_aggregate", "\nrun_fourslash_shard() {")
        set_idx = body.index("validate_emit_regression_set")
        counts_idx = body.index("validate_emit_aggregate_counts", set_idx)
        publish_idx = body.index("publish_latest_metric emit", set_idx)
        self.assertLess(set_idx, counts_idx)
        self.assertLess(counts_idx, publish_idx)

    def test_aggregate_collects_per_shard_detail_from_artifacts(self):
        body = self.function_body("run_emit_aggregate", "\nrun_fourslash_shard() {")
        self.assertIn('-name "emit-detail-*.json"', body)
        self.assertIn('cp "$detail" "$tmp_dir/detail-', body)

    def test_workflow_uploads_the_detail_the_aggregate_reads(self):
        workflow = (ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
        self.assertIn("emit-detail-${{ matrix.shard }}.json", workflow)

    def test_arch_size_prevents_same_change_rewrite_baseline_ratchet_down(self):
        workflow = (ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
        arch_size = workflow.split("\n  arch-size:\n", 1)[1].split(
            "\n  refresh-tsc-cache:\n", 1
        )[0]
        self.assertIn("Prevent rewrite emit baseline ratchet-down", arch_size)
        self.assertIn("--check-baseline-history", arch_size)
        self.assertIn("rewrite-regression-baseline.json", arch_size)

    def test_emit_aggregate_has_a_noncontinued_direction_gate(self):
        workflow = (ROOT / ".github" / "workflows" / "ci.yml").read_text(
            encoding="utf-8"
        )
        step = workflow.split("      - name: Enforce rewrite emit direction\n", 1)[
            1
        ].split("      - name:", 1)[0]
        self.assertNotIn("continue-on-error", step)
        self.assertIn("if: always()", step)
        self.assertIn("for index in 0 1 2 3", step)
        self.assertIn('"${#matches[@]}" -ne 1', step)
        self.assertIn("--require-oracle-provenance", step)
        self.assertIn("--reject-absent-baseline-rows", step)

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
            runner = temp / "run.sh"
            runner.write_text(
                "#!/usr/bin/env bash\nset -Eeuo pipefail\n%s\nvalidate_emit_regression_set '%s'\n"
                % (helper, COMMITTED_DETAIL),
                encoding="utf-8",
            )
            proc = subprocess.run(
                ["bash", str(runner)], cwd=str(ROOT), capture_output=True, text=True
            )
        # Self-comparison proves the shell helper reaches the committed rewrite
        # baseline with provenance and complete-key enforcement enabled.
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("Emit regression set OK", proc.stdout)

    def test_detail_shard_gate_requires_every_exact_shard(self):
        helper = self.function_body(
            "require_emit_detail_shards", "\nrun_emit_shard() {"
        )
        with tempfile.TemporaryDirectory() as temp_dir:
            temp = pathlib.Path(temp_dir)
            for index in range(3):
                (temp / ("detail-%d.json" % index)).write_text("{}", encoding="utf-8")
            runner = temp / "run.sh"
            runner.write_text(
                "#!/usr/bin/env bash\nset -Eeuo pipefail\n%s\n"
                "require_emit_detail_shards '%s' 4\n" % (helper, temp),
                encoding="utf-8",
            )
            proc = subprocess.run(
                ["bash", str(runner)], cwd=str(ROOT), capture_output=True, text=True
            )
        self.assertNotEqual(proc.returncode, 0)
        self.assertIn("detail shard 3/4 is missing", proc.stderr)


if __name__ == "__main__":
    unittest.main()
