"""Tests for JS/DTS emit failure-family classifier rules in query-emit.py.

Each new or modified family needs at least two name-variant cases so that the
rule is proven general rather than tied to a single test spelling.  Negative
cases confirm that unsupported shapes still fall through to "other".
"""

import importlib.util
import contextlib
import io
import json
import pathlib
import sys
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
QUERY_EMIT_PATH = ROOT / "scripts" / "emit" / "query-emit.py"


def load_query_emit():
    spec = importlib.util.spec_from_file_location("query_emit", QUERY_EMIT_PATH)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def make_result(name, js_error="", dts_error="", test_path="", baseline_file=""):
    return {
        "name": name,
        "testPath": test_path,
        "baselineFile": baseline_file,
        "jsError": js_error,
        "dtsError": dts_error,
        "jsStatus": "fail",
        "dtsStatus": "fail",
    }


class TestEmitFreshnessNote(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.mod = load_query_emit()

    def test_readme_emit_summary_parses_progress_block(self):
        summary = self.mod.emit_summary_from_readme_text(
            """
before
<!-- EMIT_START -->
```
JavaScript:  [####################] 99.5% (13,459 / 13,530 tests)
Declaration: [####################] 98.5% (1,644 / 1,669 tests)
```
<!-- EMIT_END -->
after
"""
        )
        self.assertEqual(
            summary,
            {
                "jsPass": 13459,
                "jsTotal": 13530,
                "dtsPass": 1644,
                "dtsTotal": 1669,
            },
        )

    def test_freshness_note_reports_public_aggregate_ahead(self):
        note = self.mod.emit_freshness_note(
            {
                "jsPass": 13094,
                "jsTotal": 13530,
                "dtsPass": 1606,
                "dtsTotal": 1669,
            },
            {
                "jsPass": 13459,
                "jsTotal": 13530,
                "dtsPass": 1644,
                "dtsTotal": 1669,
            },
        )
        self.assertIsNotNone(note)
        self.assertIn("README/public emit aggregate is newer", note)
        self.assertIn("JS 13,459/13,530 vs 13,094/13,530", note)
        self.assertIn("DTS 1,644/1,669 vs 1,606/1,669", note)
        self.assertIn("Pass delta: JS +365, DTS +38", note)
        self.assertIn("historical checked-detail triage only", note)
        self.assertIn("do not cite them as the current public remaining set", note)

    def test_freshness_status_reports_stale_detail(self):
        status = self.mod.emit_freshness_status(
            {
                "jsPass": 13094,
                "jsTotal": 13530,
                "dtsPass": 1606,
                "dtsTotal": 1669,
            },
            {
                "jsPass": 13459,
                "jsTotal": 13530,
                "dtsPass": 1644,
                "dtsTotal": 1669,
            },
        )
        self.assertEqual(status["state"], "stale")
        self.assertEqual(status["jsPassDelta"], 365)
        self.assertEqual(status["dtsPassDelta"], 38)

    def test_freshness_report_preserves_stale_detail_payload(self):
        report = self.mod.emit_freshness_report(
            {
                "jsPass": 13094,
                "jsTotal": 13530,
                "dtsPass": 1606,
                "dtsTotal": 1669,
            },
            {
                "jsPass": 13459,
                "jsTotal": 13530,
                "dtsPass": 1644,
                "dtsTotal": 1669,
            },
        )

        self.assertEqual(report["state"], "stale")
        self.assertFalse(report["detailIsCurrent"])
        self.assertFalse(report["detailAggregateMatchesPublic"])
        self.assertFalse(report["rowFreshnessProven"])
        self.assertEqual(report["rowFreshnessEvidence"], "stale")
        self.assertEqual(report["jsPassDelta"], 365)
        self.assertEqual(report["dtsPassDelta"], 38)
        self.assertEqual(report["detailSummary"]["jsPass"], 13094)
        self.assertEqual(report["publicSummary"]["jsPass"], 13459)
        self.assertIn("README/public ahead", report["message"])

    def test_print_freshness_json_outputs_machine_readable_status(self):
        data = {
            "summary": {
                "jsPass": 13094,
                "jsTotal": 13530,
                "dtsPass": 1606,
                "dtsTotal": 1669,
            }
        }
        original = self.mod.emit_summary_from_readme
        self.mod.emit_summary_from_readme = lambda: {
            "jsPass": 13459,
            "jsTotal": 13530,
            "dtsPass": 1644,
            "dtsTotal": 1669,
        }
        try:
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                self.mod.print_emit_freshness_json(data)
        finally:
            self.mod.emit_summary_from_readme = original

        parsed = json.loads(out.getvalue())
        self.assertEqual(parsed["state"], "stale")
        self.assertFalse(parsed["detailIsCurrent"])
        self.assertFalse(parsed["detailAggregateMatchesPublic"])
        self.assertFalse(parsed["rowFreshnessProven"])
        self.assertEqual(parsed["jsPassDelta"], 365)
        self.assertEqual(parsed["dtsPassDelta"], 38)

    def test_freshness_report_marks_aggregate_match_as_aggregate_only(self):
        summary = {
            "jsPass": 13459,
            "jsTotal": 13530,
            "dtsPass": 1644,
            "dtsTotal": 1669,
        }

        report = self.mod.emit_freshness_report(summary, dict(summary))

        self.assertEqual(report["state"], "aggregate-match")
        self.assertTrue(report["detailIsCurrent"])
        self.assertTrue(report["detailAggregateMatchesPublic"])
        self.assertFalse(report["rowFreshnessProven"])
        self.assertEqual(report["rowFreshnessEvidence"], "aggregate-only")

    def test_freshness_report_marks_matching_snapshot_fingerprint_as_row_proven(self):
        data = {
            "summary": {
                "jsPass": 1,
                "jsFail": 1,
                "jsSkip": 1,
                "jsTimeout": 0,
                "jsTotal": 2,
                "dtsPass": 2,
                "dtsFail": 1,
                "dtsSkip": 0,
                "dtsTotal": 3,
            },
            "results": [
                {
                    "name": "alpha",
                    "baselineFile": "alpha.js",
                    "testPath": "tests/cases/compiler/alpha.ts",
                    "jsStatus": "pass",
                    "dtsStatus": "pass",
                },
                {
                    "name": "beta",
                    "baselineFile": "beta.js",
                    "testPath": "tests/cases/compiler/beta.ts",
                    "jsStatus": "fail",
                    "dtsStatus": "pass",
                    "jsError": "+1/-1 lines",
                },
                {
                    "name": "gamma",
                    "baselineFile": "gamma.js",
                    "testPath": "tests/cases/compiler/gamma.ts",
                    "jsStatus": "skip",
                    "dtsStatus": "fail",
                    "dtsError": "+1/-1 lines",
                },
            ],
        }
        public_summary = self.mod.emit_summary(data)
        fingerprint = self.mod.emit_detail_row_fingerprint(data)
        original = self.mod.load_emit_snapshot
        self.mod.load_emit_snapshot = lambda: {
            "detailFingerprint": fingerprint,
            "detailResultCount": 3,
            "summary": dict(data["summary"]),
        }
        try:
            report = self.mod.emit_freshness_report(
                self.mod.emit_summary(data),
                public_summary,
                data,
            )
        finally:
            self.mod.load_emit_snapshot = original

        self.assertTrue(report["rowFreshnessProven"])
        self.assertEqual(
            report["rowFreshnessEvidence"],
            "emit-snapshot-detail-fingerprint",
        )
        self.assertTrue(report["rowFreshness"]["detailRowsMatchSummary"])
        self.assertIn("row-proven", report["message"])

    def test_freshness_report_rejects_snapshot_fingerprint_mismatch(self):
        data = {
            "summary": {
                "jsPass": 1,
                "jsFail": 0,
                "jsSkip": 0,
                "jsTimeout": 0,
                "jsTotal": 1,
                "dtsPass": 1,
                "dtsFail": 0,
                "dtsSkip": 0,
                "dtsTotal": 1,
            },
            "results": [
                {
                    "name": "alpha",
                    "baselineFile": "alpha.js",
                    "testPath": "tests/cases/compiler/alpha.ts",
                    "jsStatus": "pass",
                    "dtsStatus": "pass",
                },
            ],
        }
        original = self.mod.load_emit_snapshot
        self.mod.load_emit_snapshot = lambda: {
            "detailFingerprint": "sha256:not-the-detail",
            "detailResultCount": 1,
            "summary": dict(data["summary"]),
        }
        try:
            report = self.mod.emit_freshness_report(
                self.mod.emit_summary(data),
                self.mod.emit_summary(data),
                data,
            )
        finally:
            self.mod.load_emit_snapshot = original

        self.assertFalse(report["rowFreshnessProven"])
        self.assertEqual(
            report["rowFreshnessEvidence"],
            "snapshot-fingerprint-mismatch",
        )

    def test_detail_row_summary_counts_dts_timeout_as_failure(self):
        data = {
            "summary": {
                "jsPass": 1,
                "jsFail": 0,
                "jsSkip": 0,
                "jsTimeout": 0,
                "jsTotal": 1,
                "dtsPass": 0,
                "dtsFail": 1,
                "dtsSkip": 0,
                "dtsTotal": 1,
            },
            "results": [
                {
                    "name": "alpha",
                    "baselineFile": "alpha.js",
                    "testPath": "tests/cases/compiler/alpha.ts",
                    "jsStatus": "pass",
                    "dtsStatus": "timeout",
                    "dtsError": "TIMEOUT",
                },
            ],
        }

        self.assertEqual(self.mod.emit_detail_row_summary(data), data["summary"])
        self.assertTrue(self.mod.emit_detail_rows_match_summary(data))

    def test_terminal_artifact_states_are_failures_not_skips(self):
        results = []
        for index, status in enumerate(("unsupported", "timeout", "crash", "incomplete")):
            results.append(
                {
                    "name": f"case-{index}",
                    "baselineFile": f"case-{index}.js",
                    "testPath": f"tests/cases/compiler/case-{index}.ts",
                    "artifactState": status,
                    "jsStatus": status,
                    "dtsStatus": status,
                    "jsError": status,
                    "dtsError": status,
                }
            )
        data = {"results": results}

        summary = self.mod.emit_detail_row_summary(data)
        self.assertEqual(summary["jsFail"], 4)
        self.assertEqual(summary["jsSkip"], 0)
        self.assertEqual(summary["dtsFail"], 4)
        self.assertEqual(summary["dtsSkip"], 0)
        self.assertEqual(
            sum(len(rows) for rows in self.mod.collect_failures_by_family(data, "js").values()),
            4,
        )

    def test_artifact_state_is_part_of_detail_fingerprint(self):
        row = {
            "name": "case",
            "baselineFile": "case.js",
            "testPath": "tests/cases/compiler/case.ts",
            "artifactState": "complete",
            "jsStatus": "pass",
            "dtsStatus": "skip",
        }
        complete = self.mod.emit_detail_row_fingerprint({"results": [row]})
        incomplete = self.mod.emit_detail_row_fingerprint(
            {"results": [{**row, "artifactState": "incomplete", "jsStatus": "incomplete"}]}
        )

        self.assertNotEqual(complete, incomplete)

    def test_freshness_note_ignores_different_emit_domains(self):
        note = self.mod.emit_freshness_note(
            {
                "jsPass": 13094,
                "jsTotal": 13530,
                "dtsPass": 1606,
                "dtsTotal": 1669,
            },
            {
                "jsPass": 13459,
                "jsTotal": 14000,
                "dtsPass": 1644,
                "dtsTotal": 1669,
            },
        )
        self.assertIsNone(note)

    def test_freshness_status_reports_different_domain(self):
        status = self.mod.emit_freshness_status(
            {
                "jsPass": 13094,
                "jsTotal": 13530,
                "dtsPass": 1606,
                "dtsTotal": 1669,
            },
            {
                "jsPass": 13459,
                "jsTotal": 14000,
                "dtsPass": 1644,
                "dtsTotal": 1669,
            },
        )
        self.assertEqual(status["state"], "different-domain")
        self.assertEqual(status["jsTotalDelta"], 470)

    def test_freshness_status_reports_aggregate_match_detail(self):
        summary = {
            "jsPass": 13459,
            "jsTotal": 13530,
            "dtsPass": 1644,
            "dtsTotal": 1669,
        }
        status = self.mod.emit_freshness_status(summary, dict(summary))
        self.assertEqual(status["state"], "aggregate-match")

    def test_freshness_status_line_does_not_overstate_row_freshness(self):
        line = self.mod.emit_freshness_status_line_from_status(
            {
                "state": "aggregate-match",
                "jsPassDelta": 0,
                "dtsPassDelta": 0,
                "jsTotalDelta": 0,
                "dtsTotalDelta": 0,
            }
        )

        self.assertIn("aggregate-match", line)
        self.assertIn("per-row freshness is not proven", line)
        self.assertNotIn("current", line)

    def test_current_detail_requirement_requires_matching_aggregates(self):
        self.assertTrue(self.mod.emit_detail_is_current({"state": "aggregate-match"}))
        for state in (
            "current",
            "stale",
            "detail-ahead",
            "different-domain",
            "unknown-public",
            "missing-detail",
        ):
            with self.subTest(state=state):
                self.assertFalse(self.mod.emit_detail_is_current({"state": state}))

    def test_rewrite_readme_makes_no_current_emit_aggregate_claim(self):
        """R0 keeps broad emit artifacts observational and fail-closed.

        The only README numbers are explicitly frozen legacy history. They must
        not be parsed as a current aggregate or used to validate/refresh the
        clean-slate rewrite's committed detail artifact.
        """
        self.assertIsNone(self.mod.emit_summary_from_readme())

    def test_stale_failure_family_heading_names_public_remaining_count(self):
        detail_summary = {
            "jsPass": 13094,
            "jsTotal": 13530,
            "dtsPass": 1606,
            "dtsTotal": 1669,
        }
        public_summary = {
            "jsPass": 13459,
            "jsTotal": 13530,
            "dtsPass": 1644,
            "dtsTotal": 1669,
        }

        heading = self.mod.failure_family_surface_heading(
            "js", "JavaScript", 436, detail_summary, public_summary
        )

        self.assertIn("JavaScript STALE checked-detail triage: 436 failures/timeouts", heading)
        self.assertIn("public aggregate remaining: 71", heading)
        self.assertIn("detail aggregate remaining: 436", heading)

    def test_stale_failure_families_are_suppressed_by_default(self):
        data = {
            "summary": {
                "jsPass": 13094,
                "jsTotal": 13530,
                "dtsPass": 1606,
                "dtsTotal": 1669,
            },
            "results": [
                make_result("classUsedBeforeInitializedVariables", js_error="+1/-1 lines"),
                make_result("inferTypePredicates", dts_error="+1/-1 lines"),
            ],
        }
        original = self.mod.emit_summary_from_readme
        self.mod.emit_summary_from_readme = lambda: {
            "jsPass": 13459,
            "jsTotal": 13530,
            "dtsPass": 1644,
            "dtsTotal": 1669,
        }
        try:
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                self.mod.show_failure_families(data)
        finally:
            self.mod.emit_summary_from_readme = original

        text = out.getvalue()
        self.assertIn("Failure-family rows are suppressed", text)
        self.assertIn("JavaScript: public aggregate remaining 71", text)
        self.assertIn("Declaration: public aggregate remaining 25", text)
        self.assertIn("--include-stale-detail", text)
        self.assertNotIn("class/private/accessor/decorator lowering", text)
        self.assertNotIn("generic/type-display declarations", text)

    def test_stale_failure_families_can_be_included_explicitly(self):
        data = {
            "summary": {
                "jsPass": 13094,
                "jsTotal": 13530,
                "dtsPass": 1606,
                "dtsTotal": 1669,
            },
            "results": [
                make_result("classUsedBeforeInitializedVariables", js_error="+1/-1 lines"),
                make_result("inferTypePredicates", dts_error="+1/-1 lines"),
            ],
        }
        original = self.mod.emit_summary_from_readme
        self.mod.emit_summary_from_readme = lambda: {
            "jsPass": 13459,
            "jsTotal": 13530,
            "dtsPass": 1644,
            "dtsTotal": 1669,
        }
        try:
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                self.mod.show_failure_families(data, include_stale_detail=True)
        finally:
            self.mod.emit_summary_from_readme = original

        text = out.getvalue()
        self.assertIn("historical checked-detail triage only", text)
        self.assertIn("class/private/accessor/decorator lowering", text)
        self.assertIn("generic/type-display declarations", text)

    def test_failure_family_json_suppresses_stale_rows_by_default(self):
        data = {
            "summary": {
                "jsPass": 13094,
                "jsTotal": 13530,
                "dtsPass": 1606,
                "dtsTotal": 1669,
            },
            "results": [
                make_result("classUsedBeforeInitializedVariables", js_error="+1/-1 lines"),
                make_result("inferTypePredicates", dts_error="+1/-1 lines"),
            ],
        }
        original = self.mod.emit_summary_from_readme
        self.mod.emit_summary_from_readme = lambda: {
            "jsPass": 13459,
            "jsTotal": 13530,
            "dtsPass": 1644,
            "dtsTotal": 1669,
        }
        try:
            report = self.mod.failure_family_report(data)
        finally:
            self.mod.emit_summary_from_readme = original

        self.assertEqual(report["freshness"]["state"], "stale")
        self.assertTrue(report["familiesSuppressed"])
        self.assertFalse(report["includeStaleDetail"])
        self.assertEqual(
            [
                (surface["surface"], surface["publicRemaining"], surface["checkedDetailRemaining"])
                for surface in report["surfaces"]
            ],
            [("js", 71, 436), ("dts", 25, 63)],
        )
        self.assertEqual(report["surfaces"][0]["families"], [])
        self.assertIsNone(report["surfaces"][0]["checkedDetailFailures"])

    def test_failure_family_json_includes_explicit_stale_rows(self):
        data = {
            "summary": {
                "jsPass": 13094,
                "jsTotal": 13530,
                "dtsPass": 1606,
                "dtsTotal": 1669,
            },
            "results": [
                {
                    **make_result("classUsedBeforeInitializedVariables", js_error="+1/-1 lines"),
                    "dtsStatus": "pass",
                },
                {
                    **make_result("classWithStaticBlock", js_error="+1/-1 lines"),
                    "dtsStatus": "pass",
                },
                {
                    **make_result("inferTypePredicates", dts_error="+1/-1 lines"),
                    "jsStatus": "pass",
                },
            ],
        }
        original = self.mod.emit_summary_from_readme
        self.mod.emit_summary_from_readme = lambda: {
            "jsPass": 13459,
            "jsTotal": 13530,
            "dtsPass": 1644,
            "dtsTotal": 1669,
        }
        try:
            report = self.mod.failure_family_report(
                data,
                top=1,
                include_stale_detail=True,
            )
        finally:
            self.mod.emit_summary_from_readme = original

        self.assertFalse(report["familiesSuppressed"])
        self.assertTrue(report["includeStaleDetail"])
        self.assertEqual(report["top"], 1)
        self.assertEqual(report["surfaces"][0]["checkedDetailFailures"], 2)
        self.assertEqual(
            report["surfaces"][0]["families"],
            [
                {
                    "family": "class/private/accessor/decorator lowering",
                    "count": 2,
                    "examples": [
                        "classUsedBeforeInitializedVariables",
                        "classWithStaticBlock",
                    ],
                }
            ],
        )
        self.assertEqual(report["surfaces"][1]["checkedDetailFailures"], 1)
        self.assertEqual(
            report["surfaces"][1]["families"][0]["family"],
            "generic/type-display declarations",
        )

    def test_current_failure_family_heading_keeps_plain_count(self):
        summary = {
            "jsPass": 13459,
            "jsTotal": 13530,
            "dtsPass": 1644,
            "dtsTotal": 1669,
        }

        heading = self.mod.failure_family_surface_heading(
            "dts", "Declaration", 25, summary, dict(summary)
        )

        self.assertEqual(heading, "Declaration: 25 failures/timeouts")

    def test_aggregate_match_failure_families_warn_row_freshness_unproven(self):
        summary = {
            "jsPass": 13459,
            "jsTotal": 13530,
            "dtsPass": 1644,
            "dtsTotal": 1669,
        }
        data = {
            "summary": dict(summary),
            "results": [
                make_result("typeGuardsInFunction", js_error="+1/-1 lines"),
            ],
        }
        original = self.mod.emit_summary_from_readme
        self.mod.emit_summary_from_readme = lambda: dict(summary)
        try:
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                self.mod.show_failure_families(data)
        finally:
            self.mod.emit_summary_from_readme = original

        text = out.getvalue()
        self.assertIn("Emit detail freshness: aggregate-match", text)
        self.assertIn("per-row freshness is not proven", text)
        self.assertIn("JavaScript: 1 failures/timeouts", text)

    def test_filtered_family_json_uses_full_detail_for_row_freshness(self):
        data = {
            "summary": {
                "jsPass": 1,
                "jsFail": 1,
                "jsSkip": 0,
                "jsTimeout": 0,
                "jsTotal": 2,
                "dtsPass": 2,
                "dtsFail": 0,
                "dtsSkip": 0,
                "dtsTotal": 2,
            },
            "results": [
                {
                    **make_result("classUsedBeforeInitializedVariables", js_error="+1/-1 lines"),
                    "dtsStatus": "pass",
                },
                {
                    **make_result("mappedTypeProperties"),
                    "jsStatus": "pass",
                    "dtsStatus": "pass",
                    "jsError": "",
                    "dtsError": "",
                },
            ],
        }
        fingerprint = self.mod.emit_detail_row_fingerprint(data)
        original_argv = sys.argv
        original_load_detail = self.mod.load_detail
        original_readme = self.mod.emit_summary_from_readme
        original_snapshot = self.mod.load_emit_snapshot
        self.mod.load_detail = lambda: data
        self.mod.emit_summary_from_readme = lambda: self.mod.emit_summary(data)
        self.mod.load_emit_snapshot = lambda: {
            "detailFingerprint": fingerprint,
            "detailResultCount": 2,
            "summary": dict(data["summary"]),
        }
        sys.argv = [
            "query-emit.py",
            "--families-json",
            "--filter",
            "class",
        ]
        try:
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                rc = self.mod.main()
        finally:
            sys.argv = original_argv
            self.mod.load_detail = original_load_detail
            self.mod.emit_summary_from_readme = original_readme
            self.mod.load_emit_snapshot = original_snapshot

        self.assertEqual(rc, 0)
        report = json.loads(out.getvalue())
        self.assertTrue(report["freshness"]["rowFreshnessProven"])
        self.assertEqual(
            report["freshness"]["rowFreshnessEvidence"],
            "emit-snapshot-detail-fingerprint",
        )
        self.assertEqual(report["freshness"]["rowFreshness"]["detailResultCount"], 2)
        self.assertEqual(report["surfaces"][0]["checkedDetailFailures"], 1)
        self.assertEqual(
            report["surfaces"][0]["families"][0]["examples"],
            ["classUsedBeforeInitializedVariables"],
        )

    def test_stale_headline_summary_uses_public_aggregate(self):
        detail_summary = {
            "jsPass": 13094,
            "jsTotal": 13530,
            "dtsPass": 1606,
            "dtsTotal": 1669,
        }
        public_summary = {
            "jsPass": 13459,
            "jsTotal": 13530,
            "dtsPass": 1644,
            "dtsTotal": 1669,
        }

        summary, source = self.mod.emit_headline_summary(detail_summary, public_summary)

        self.assertEqual(source, "README/public aggregate")
        self.assertEqual(summary["jsPass"], 13459)
        self.assertEqual(summary["dtsPass"], 1644)

    def test_current_headline_summary_uses_checked_detail(self):
        detail_summary = {
            "jsPass": 13459,
            "jsTotal": 13530,
            "dtsPass": 1644,
            "dtsTotal": 1669,
        }
        public_summary = dict(detail_summary)

        summary, source = self.mod.emit_headline_summary(detail_summary, public_summary)

        self.assertEqual(source, "checked detail")
        self.assertEqual(summary, detail_summary)

    def test_stale_overview_labels_checked_detail_counters(self):
        data = {
            "summary": {
                "jsPass": 13094,
                "jsTotal": 13530,
                "jsPassRate": 96.8,
                "dtsPass": 1606,
                "dtsTotal": 1669,
                "dtsPassRate": 96.2,
            },
            "results": [
                {
                    "name": "jsOnlyFailure",
                    "jsStatus": "fail",
                    "dtsStatus": "pass",
                    "jsError": "+1/-1 lines",
                },
                {
                    "name": "dtsOnlyFailure",
                    "jsStatus": "pass",
                    "dtsStatus": "fail",
                    "dtsError": "+1/-1 lines",
                },
            ],
        }
        original = self.mod.emit_summary_from_readme
        self.mod.emit_summary_from_readme = lambda: {
            "jsPass": 13459,
            "jsTotal": 13530,
            "dtsPass": 1644,
            "dtsTotal": 1669,
        }
        try:
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                self.mod.show_overview(data)
        finally:
            self.mod.emit_summary_from_readme = original

        text = out.getvalue()
        self.assertIn("Source: README/public aggregate", text)
        self.assertIn("JavaScript: 13459/13530 (99.5%)", text)
        self.assertIn("Checked-detail JS failures: 1", text)
        self.assertIn("Checked-detail DTS failures: 1", text)
        self.assertIn("Checked-detail JS pass + DTS fail", text)


class TestQueryFilters(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.mod = load_query_emit()

    def test_name_filter_scopes_dts_failures(self):
        data = {
            "results": [
                make_result("checkJsdocSatisfiesTag15", dts_error="+1/-1 lines"),
                make_result("jsDeclarationsClasses", dts_error="+21/-17 lines"),
            ]
        }
        filtered = self.mod.filter_data_by_name(data, "checkJsdoc")

        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.mod.show_dts_failures(filtered)

        text = out.getvalue()
        self.assertIn("DTS failures: 1", text)
        self.assertIn("checkJsdocSatisfiesTag15", text)
        self.assertNotIn("jsDeclarationsClasses", text)

    def test_name_filter_scopes_paths_only_failure_output(self):
        data = {
            "results": [
                make_result("checkJsdocSatisfiesTag15", dts_error="+1/-1 lines"),
                make_result("jsDeclarationsClasses", dts_error="+21/-17 lines"),
            ]
        }
        filtered = self.mod.filter_data_by_name(data, "jsDeclarations")

        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.mod.show_dts_failures(filtered, paths_only=True)

        self.assertEqual(out.getvalue(), "jsDeclarationsClasses\n")


class TestJSFamilyClassifier(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.mod = load_query_emit()

    def classify(self, name, js_error=""):
        result = make_result(name, js_error=js_error)
        return self.mod.classify_failure(result, "js")

    # --- parser/recovery emit ---

    def test_parser_prefix_catches_computed_property(self):
        self.assertEqual(self.classify("parserComputedPropertyName25"), "parser/recovery emit")

    def test_parser_prefix_catches_skipped_tokens(self):
        self.assertEqual(self.classify("parserSkippedTokens13"), "parser/recovery emit")

    def test_parser_prefix_catches_real_source(self):
        self.assertEqual(self.classify("parserRealSource10"), "parser/recovery emit")

    def test_parse_bigint_needle_catches_parseBigInt(self):
        self.assertEqual(self.classify("parseBigInt"), "parser/recovery emit")

    def test_parse_error_needle_catches_parseErrorIncorrectReturnToken(self):
        self.assertEqual(self.classify("parseErrorIncorrectReturnToken"), "parser/recovery emit")

    def test_parse_invalid_needle_catches_parseInvalidNames(self):
        self.assertEqual(self.classify("parseInvalidNames"), "parser/recovery emit")

    def test_parse_assert_needle_catches_parseAssertEntriesError(self):
        self.assertEqual(self.classify("parseAssertEntriesError"), "parser/recovery emit")

    def test_skippedtoken_needle_variant(self):
        self.assertEqual(self.classify("parserSkippedTokens8"), "parser/recovery emit")

    # --- type-guard emit ---

    def test_typeguard_prefix_catches_typeGuardFunctionErrors(self):
        self.assertEqual(self.classify("typeGuardFunctionErrors"), "type-guard emit")

    def test_typeguards_prefix_catches_typeGuardsInConditionalExpression(self):
        self.assertEqual(
            self.classify("typeGuardsInConditionalExpression"), "type-guard emit"
        )

    def test_typeguards_right_operand_and(self):
        self.assertEqual(
            self.classify("typeGuardsInRightOperandOfAndAndOperator"), "type-guard emit"
        )

    def test_typeguards_right_operand_or(self):
        self.assertEqual(
            self.classify("typeGuardsInRightOperandOfOrOrOperator"), "type-guard emit"
        )

    def test_typepredicate_needle_catches_inferTypePredicates(self):
        # "typepredicate" is a substring of "infertypepredicates"
        self.assertEqual(self.classify("inferTypePredicates"), "type-guard emit")

    # --- optional-chain/nullish emit ---

    def test_chain_catches_elementAccessChain(self):
        self.assertEqual(self.classify("elementAccessChain.3"), "optional-chain/nullish emit")

    def test_chain_catches_propertyAccessChain(self):
        self.assertEqual(self.classify("propertyAccessChain.3"), "optional-chain/nullish emit")

    def test_optionalchaining_catches_optionalChainingInArrow(self):
        self.assertEqual(
            self.classify("optionalChainingInArrow(target=es5)"), "optional-chain/nullish emit"
        )

    def test_optionalchaining_catches_optionalChainingInLoop(self):
        self.assertEqual(
            self.classify("optionalChainingInLoop(target=es5)"), "optional-chain/nullish emit"
        )

    def test_chain_catches_invalidOptionalChainFromNewExpression(self):
        self.assertEqual(
            self.classify("invalidOptionalChainFromNewExpression"), "optional-chain/nullish emit"
        )

    def test_chain_catches_genericChainedCalls(self):
        self.assertEqual(self.classify("genericChainedCalls"), "optional-chain/nullish emit")

    # --- unicode/identifier-encoding emit ---

    def test_unicode_catches_invalidUnicodeEscapeSequance(self):
        self.assertEqual(
            self.classify("invalidUnicodeEscapeSequance"), "unicode/identifier-encoding emit"
        )

    def test_unicode_catches_invalidUnicodeEscapeSequance2(self):
        self.assertEqual(
            self.classify("invalidUnicodeEscapeSequance2"), "unicode/identifier-encoding emit"
        )

    def test_unicode_catches_unicodeEscapesInNames(self):
        self.assertEqual(
            self.classify("unicodeEscapesInNames02(target=es5)"), "unicode/identifier-encoding emit"
        )

    # --- reserved-word emit ---

    def test_reservedword_catches_reservedWords2(self):
        self.assertEqual(self.classify("reservedWords2"), "reserved-word emit")

    def test_reservedword_catches_reservedWords3(self):
        self.assertEqual(self.classify("reservedWords3"), "reserved-word emit")

    def test_reservedname_catches_reservedNamesInAliases(self):
        self.assertEqual(self.classify("reservedNamesInAliases"), "reserved-word emit")

    # --- js-file/plain-js emit ---

    def test_jsdeclaration_catches_jsDeclarationsNestedParams(self):
        self.assertEqual(self.classify("jsDeclarationsNestedParams"), "js-file/plain-js emit")

    def test_jsdeclaration_catches_jsDeclarationsTypeReferences4(self):
        self.assertEqual(
            self.classify("jsDeclarationsTypeReferences4(target=es5)"), "js-file/plain-js emit"
        )

    def test_jsfile_catches_jsFileCompilationEmitTrippleSlashReference(self):
        self.assertEqual(
            self.classify("jsFileCompilationEmitTrippleSlashReference"), "js-file/plain-js emit"
        )

    def test_jsfile_catches_jsFileCompilationTypeArgumentSyntaxOfCall(self):
        self.assertEqual(
            self.classify("jsFileCompilationTypeArgumentSyntaxOfCall"), "js-file/plain-js emit"
        )

    def test_plainjsgrammar_catches_plainJSGrammarErrors(self):
        self.assertEqual(self.classify("plainJSGrammarErrors"), "js-file/plain-js emit")

    # --- new-target emit ---

    def test_newtarget_catches_newTarget_es5(self):
        self.assertEqual(self.classify("newTarget.es5(target=es5)"), "new-target emit")

    def test_newtarget_catches_invalidNewTarget_es5(self):
        self.assertEqual(self.classify("invalidNewTarget.es5(target=es5)"), "new-target emit")

    # --- tslib/helper emit ---

    def test_tslib_catches_tslibMissingHelper(self):
        self.assertEqual(self.classify("tslibMissingHelper"), "tslib/helper emit")

    def test_tslib_catches_tslibMultipleMissingHelper(self):
        self.assertEqual(self.classify("tslibMultipleMissingHelper"), "tslib/helper emit")

    # --- jsdoc-type emit ---

    def test_jsdoc_catches_expressionWithJSDocTypeArguments(self):
        self.assertEqual(self.classify("expressionWithJSDocTypeArguments"), "jsdoc-type emit")

    # --- tsx extension added to jsx/react family ---

    def test_tsx_catches_tsxStatelessComponentDefaultProps(self):
        self.assertEqual(self.classify("tsxStatelessComponentDefaultProps"), "jsx/react emit")

    def test_tsx_catches_tsxUnionMemberChecksFilterDataProps(self):
        self.assertEqual(self.classify("tsxUnionMemberChecksFilterDataProps"), "jsx/react emit")

    # --- Rule ordering: existing rules take priority over new ones ---

    def test_existing_async_rule_not_overridden_by_parser(self):
        # Existing rules must fire before the new extended families (order check).
        result = self.classify("asyncGeneratorParameterEvaluation(target=es2015)")
        self.assertEqual(result, "async/await/generator lowering")

    def test_existing_class_rule_not_overridden_by_chain(self):
        result = self.classify("classStaticBlock18(target=es5)")
        self.assertEqual(result, "class/private/accessor/decorator lowering")

    # --- "other" as the final fallback ---

    def test_truly_unclassified_falls_to_other(self):
        # Avoid names that contain existing needles as substrings (e.g. "let" in "completely").
        self.assertEqual(self.classify("unknownXyzAbcDef99"), "other")

    def test_giant_test_still_other(self):
        self.assertEqual(self.classify("giant"), "other")


class TestDTSFamilyClassifier(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.mod = load_query_emit()

    def classify(self, name, dts_error=""):
        result = make_result(name, dts_error=dts_error)
        return self.mod.classify_failure(result, "dts")

    # --- jsdoc/javascript declarations: new jsfile needle ---

    def test_jsfile_catches_jsFileAlternativeUseOfOverloadTag(self):
        self.assertEqual(
            self.classify("jsFileAlternativeUseOfOverloadTag"), "jsdoc/javascript declarations"
        )

    def test_jsfile_catches_jsFileCompilationDuplicateVariable(self):
        self.assertEqual(
            self.classify("jsFileCompilationDuplicateVariable"), "jsdoc/javascript declarations"
        )

    def test_jsfile_catches_jsFileFunctionOverloads(self):
        self.assertEqual(
            self.classify("jsFileFunctionOverloads"), "jsdoc/javascript declarations"
        )

    def test_jsfile_catches_jsFileMethodOverloads2(self):
        self.assertEqual(
            self.classify("jsFileMethodOverloads2"), "jsdoc/javascript declarations"
        )

    # --- module/declaration merging: new symlink/moduledecl/nodemodule needles ---

    def test_symlink_catches_symlinkedWorkspaceDependencies(self):
        self.assertEqual(
            self.classify("symlinkedWorkspaceDependenciesNoDirectLinkGeneratesNonrelativeName"),
            "module/declaration merging",
        )

    def test_symlink_catches_symlinkedWorkspaceOptional(self):
        self.assertEqual(
            self.classify(
                "symlinkedWorkspaceDependenciesNoDirectLinkOptionalGeneratesNonrelativeName"
            ),
            "module/declaration merging",
        )

    def test_moduledecl_catches_moduledecl_es2015(self):
        self.assertEqual(
            self.classify("moduledecl(target=es2015)"), "module/declaration merging"
        )

    def test_moduledecl_catches_moduledecl_es5(self):
        self.assertEqual(
            self.classify("moduledecl(target=es5)"), "module/declaration merging"
        )

    def test_nodemodule_catches_nodeModulesResolveJsonModule(self):
        self.assertEqual(
            self.classify("nodeModulesResolveJsonModule(module=node16)"),
            "module/declaration merging",
        )

    def test_nodemodule_catches_nodeModulesResolveJsonModule_nodenext(self):
        self.assertEqual(
            self.classify("nodeModulesResolveJsonModule(module=nodenext)"),
            "module/declaration merging",
        )

    # --- class/private/accessor declarations: new privacy needle ---

    def test_privacy_catches_privacyCheckAnonymousFunctionParameter(self):
        self.assertEqual(
            self.classify("privacyCheckAnonymousFunctionParameter"),
            "class/private/accessor declarations",
        )

    def test_privacy_catches_privacyCheckAnonymousFunctionParameter2(self):
        self.assertEqual(
            self.classify("privacyCheckAnonymousFunctionParameter2"),
            "class/private/accessor declarations",
        )

    def test_privacy_catches_privacyFunctionReturnTypeDeclFile(self):
        self.assertEqual(
            self.classify("privacyFunctionReturnTypeDeclFile"),
            "class/private/accessor declarations",
        )

    # --- generic/type-display declarations: extended needles ---

    def test_template_catches_templateLiteralTypes2(self):
        self.assertEqual(
            self.classify("templateLiteralTypes2"), "generic/type-display declarations"
        )

    def test_template_catches_templateLiteralTypes4(self):
        self.assertEqual(
            self.classify("templateLiteralTypes4"), "generic/type-display declarations"
        )

    def test_variadic_catches_variadicTuples1(self):
        self.assertEqual(
            self.classify("variadicTuples1"), "generic/type-display declarations"
        )

    def test_variadic_catches_variadicTuples2(self):
        self.assertEqual(
            self.classify("variadicTuples2"), "generic/type-display declarations"
        )

    def test_tuple_catches_restTuplesFromContextualTypes(self):
        self.assertEqual(
            self.classify("restTuplesFromContextualTypes"), "generic/type-display declarations"
        )

    def test_tuple_catches_namedTupleMembers(self):
        self.assertEqual(
            self.classify("namedTupleMembers"), "generic/type-display declarations"
        )

    def test_stringliteral_catches_stringLiteralTypesOverloads01(self):
        self.assertEqual(
            self.classify("stringLiteralTypesOverloads01"), "generic/type-display declarations"
        )

    def test_stringliteral_catches_stringLiteralTypesAndTuples01(self):
        self.assertEqual(
            self.classify("stringLiteralTypesAndTuples01"), "generic/type-display declarations"
        )

    def test_spread_catches_spreadDuplicate(self):
        self.assertEqual(self.classify("spreadDuplicate"), "generic/type-display declarations")

    def test_spread_catches_spreadObjectOrFalsy(self):
        self.assertEqual(
            self.classify("spreadObjectOrFalsy"), "generic/type-display declarations"
        )

    def test_never_catches_neverType(self):
        self.assertEqual(self.classify("neverType"), "generic/type-display declarations")

    def test_never_catches_silentNeverPropagation(self):
        self.assertEqual(
            self.classify("silentNeverPropagation"), "generic/type-display declarations"
        )

    def test_noimplicit_catches_noImplicitThisBigThis(self):
        self.assertEqual(
            self.classify("noImplicitThisBigThis"), "generic/type-display declarations"
        )

    # --- type-guard declarations (new family) ---

    def test_typeguard_catches_typeGuardFunctionOfFormThis(self):
        self.assertEqual(
            self.classify("typeGuardFunctionOfFormThis"), "type-guard declarations"
        )

    def test_typeguard_catches_typeGuardOfFormThisMember(self):
        self.assertEqual(
            self.classify("typeGuardOfFormThisMember(target=es2015)"), "type-guard declarations"
        )

    def test_typeguard_catches_typeGuardOfFormThisMember_es5(self):
        self.assertEqual(
            self.classify("typeGuardOfFormThisMember(target=es5)"), "type-guard declarations"
        )

    # --- unique-symbol declarations (new family) ---

    def test_uniquesymbol_catches_uniqueSymbolsDeclarations(self):
        self.assertEqual(
            self.classify("uniqueSymbolsDeclarations"), "unique-symbol declarations"
        )

    # --- "other" as the final fallback ---

    def test_giant_still_other_in_dts(self):
        self.assertEqual(self.classify("giant"), "other")

    def test_truly_unclassified_falls_to_other_dts(self):
        self.assertEqual(self.classify("unknownXyzAbcDef99"), "other")


if __name__ == "__main__":
    unittest.main()
