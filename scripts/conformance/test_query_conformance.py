import contextlib
import importlib.util
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).with_name("query-conformance.py")
SPEC = importlib.util.spec_from_file_location("query_conformance", SCRIPT_PATH)
query_conformance = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = query_conformance
SPEC.loader.exec_module(query_conformance)


def dashboard_data(failures):
    return {
        "summary": {
            "passed": 10 - len(failures),
            "total": 10,
        },
        "failures": failures,
        "aggregates": {
            "categories": {},
            "areas_by_pass_rate": [],
        },
    }


class QueryConformanceDashboardTests(unittest.TestCase):
    def render_dashboard(
        self,
        data,
        accepted_text="",
        tsc_cache_text=None,
        domain_text=None,
    ):
        with tempfile.TemporaryDirectory() as tmp:
            accepted_path = Path(tmp) / "accepted.txt"
            accepted_path.write_text(accepted_text, encoding="utf-8")
            tsc_cache_path = Path(tmp) / "tsc-cache-full.json"
            if tsc_cache_text is not None:
                tsc_cache_path.write_text(tsc_cache_text, encoding="utf-8")
            domain_path = Path(tmp) / "conformance-domain.json"
            if domain_text is not None:
                domain_path.write_text(domain_text, encoding="utf-8")
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                query_conformance.show_dashboard(
                    data,
                    accepted_regressions_path=str(accepted_path),
                    tsc_cache_path=tsc_cache_path,
                    domain_path=domain_path,
                )
            return output.getvalue()

    def test_zero_failure_dashboard_does_not_report_stale_fingerprint_share(self):
        output = self.render_dashboard(
            dashboard_data({}),
            "# comment\n\nTypeScript/tests/cases/compiler/example.ts\n",
        )

        self.assertIn("Overall: 10/10 (100.0%)", output)
        self.assertIn("Accepted-regression gate: 1 listed tests", output)
        self.assertIn("No conformance failures remain in the current detail snapshot.", output)
        self.assertIn("Accepted-regression strictness still lists 1 tests.", output)
        self.assertNotIn("Fingerprint parity is 73.6% of remaining work.", output)

    def test_nonzero_dashboard_reports_current_fingerprint_share(self):
        output = self.render_dashboard(
            dashboard_data(
                {
                    "TypeScript/tests/cases/compiler/fingerprint.ts": {
                        "e": ["TS2322"],
                        "a": ["TS2322"],
                    },
                    "TypeScript/tests/cases/compiler/wrong-code.ts": {
                        "e": ["TS2322"],
                        "a": ["TS2345"],
                        "m": ["TS2322"],
                        "x": ["TS2345"],
                    },
                }
            ),
        )

        self.assertIn("Overall: 8/10 (80.0%)", output)
        self.assertIn("Accepted-regression gate: 0 listed tests", output)
        self.assertIn("Fingerprint-only failures: 1/2 (50.0% of current failures).", output)
        self.assertNotIn("No conformance failures remain", output)

    def test_dashboard_warns_when_snapshot_total_lags_tsc_cache(self):
        output = self.render_dashboard(
            dashboard_data({}),
            tsc_cache_text=json.dumps({f"{index}.ts": {} for index in range(11)}),
        )

        self.assertIn("Overall: 10/10 (100.0%)", output)
        self.assertIn(
            "Snapshot freshness: STALE checked detail covers 10 runnable tests",
            output,
        )
        self.assertIn(
            "pinned TypeScript cache has 11 runnable tests (delta +1)",
            output,
        )
        self.assertIn("Refresh conformance-detail.json before citing this dashboard", output)

    def test_dashboard_freshness_stays_quiet_when_totals_match(self):
        output = self.render_dashboard(
            dashboard_data({}),
            tsc_cache_text=json.dumps({f"{index}.ts": {} for index in range(10)}),
        )

        self.assertNotIn("Snapshot freshness", output)

    def test_dashboard_uses_candidates_for_freshness_and_runnable_for_rate(self):
        data = dashboard_data({})
        data["summary"].update(
            {
                "candidates": 12,
                "runnable": 10,
                "unsupported": 1,
                "skipped": 1,
            }
        )
        output = self.render_dashboard(
            data,
            tsc_cache_text=json.dumps({f"{index}.ts": {} for index in range(10)}),
            domain_text=json.dumps(
                {
                    "candidate_count": 12,
                    "runnable_count": 10,
                    "unsupported_count": 1,
                    "skipped_count": 1,
                }
            ),
        )

        self.assertIn("Overall: 10/10 (100.0%)", output)
        self.assertIn(
            "Candidates: 12 (10 runnable, 1 unsupported, 1 skipped)",
            output,
        )
        self.assertNotIn("Snapshot freshness", output)

    def test_dashboard_reports_candidate_drift_from_domain_manifest(self):
        data = dashboard_data({})
        data["summary"].update({"candidates": 12, "runnable": 10})

        output = self.render_dashboard(
            data,
            domain_text=json.dumps({"candidate_count": 13}),
        )

        self.assertIn(
            "checked detail covers 12 candidates, but the checked conformance domain "
            "has 13 candidates (delta +1)",
            output,
        )

    def test_dashboard_handles_an_all_unsupported_candidate_set(self):
        data = dashboard_data({})
        data["summary"].update(
            {
                "passed": 0,
                "total": 0,
                "candidates": 1,
                "runnable": 0,
                "unsupported": 1,
            }
        )

        output = self.render_dashboard(data)

        self.assertIn("Overall: 0/0 (0.0%)", output)
        self.assertIn(
            "Candidates: 1 (0 runnable, 1 unsupported, 0 skipped)",
            output,
        )


class QueryConformanceReadmeFreshnessTests(unittest.TestCase):
    """Tests for the README-vs-detail freshness check.

    Distinct from show_snapshot_freshness (detail vs. checked TypeScript
    corpus domain): this compares conformance-detail.json's summary against
    what README.md publicly claims, mirroring
    scripts/emit/query-emit.py's emit_freshness_status family for emit.
    """

    def test_readme_parser_reads_plain_progress_line(self):
        summary = query_conformance.conformance_summary_from_readme_text(
            """<!-- CONFORMANCE_START -->
```
Progress: [███████████████████░] 90.0% (9/10 tests)
```
<!-- CONFORMANCE_END -->""",
        )

        self.assertEqual(summary, {"passed": 9, "runnable": 10})

    def test_readme_parser_reads_candidate_partition(self):
        summary = query_conformance.conformance_summary_from_readme_text(
            """<!-- CONFORMANCE_START -->
```
Progress: [████████████████████] 100.0% (8/8 runnable tests)
Candidates: 10 (8 runnable, 1 unsupported, 1 skipped)
```
<!-- CONFORMANCE_END -->""",
        )

        self.assertEqual(
            summary,
            {
                "passed": 8,
                "runnable": 8,
                "candidates": 10,
                "unsupported": 1,
                "skipped": 1,
            },
        )

    def test_readme_parser_returns_none_without_markers(self):
        self.assertIsNone(query_conformance.conformance_summary_from_readme_text("no markers here"))

    def test_readme_parser_rejects_unmatched_or_duplicate_markers(self):
        valid_claim = "Progress: 100.0% (10/10 tests)"
        for text in (
            f"<!-- CONFORMANCE_START -->\n{valid_claim}",
            f"{valid_claim}\n<!-- CONFORMANCE_END -->",
            (
                "<!-- CONFORMANCE_START -->\n"
                f"{valid_claim}\n"
                "<!-- CONFORMANCE_END -->\n"
                "<!-- CONFORMANCE_END -->"
            ),
            (
                "<!-- CONFORMANCE_START -->\n"
                "<!-- CONFORMANCE_START -->\n"
                f"{valid_claim}\n"
                "<!-- CONFORMANCE_END -->"
            ),
        ):
            self.assertIsNone(
                query_conformance.conformance_summary_from_readme_text(text), text
            )

    def test_freshness_status_reports_stale_when_readme_leads_detail(self):
        status = query_conformance.conformance_readme_freshness_status(
            {"passed": 11430, "runnable": 12043},
            {"passed": 11435, "runnable": 12043},
        )

        self.assertEqual(status["state"], "stale")
        self.assertEqual(status["passedDelta"], 5)

    def test_freshness_status_reports_detail_ahead_when_detail_leads_readme(self):
        status = query_conformance.conformance_readme_freshness_status(
            {"passed": 11435, "runnable": 12043},
            {"passed": 11430, "runnable": 12043},
        )

        self.assertEqual(status["state"], "detail-ahead")
        self.assertEqual(status["passedDelta"], -5)

    def test_freshness_status_reports_aggregate_match(self):
        summary = {"passed": 11435, "runnable": 12043}
        status = query_conformance.conformance_readme_freshness_status(summary, dict(summary))

        self.assertEqual(status["state"], "aggregate-match")
        self.assertTrue(query_conformance.conformance_readme_detail_is_current(status))

    def test_freshness_status_reports_different_domain(self):
        status = query_conformance.conformance_readme_freshness_status(
            {"passed": 11435, "runnable": 12043},
            {"passed": 11435, "runnable": 12000},
        )

        self.assertEqual(status["state"], "different-domain")
        self.assertEqual(status["runnableDelta"], -43)
        self.assertFalse(query_conformance.conformance_readme_detail_is_current(status))

    def test_freshness_status_reports_missing_or_unknown(self):
        self.assertEqual(
            query_conformance.conformance_readme_freshness_status(None, {"passed": 1, "runnable": 1})["state"],
            "missing-detail",
        )
        self.assertEqual(
            query_conformance.conformance_readme_freshness_status({"passed": 1, "runnable": 1}, None)["state"],
            "unknown-public",
        )

    def test_require_current_readme_exits_nonzero_when_stale(self):
        old_load_detail = query_conformance.load_detail
        old_summary_from_readme = query_conformance.conformance_summary_from_readme
        old_argv = sys.argv
        try:
            query_conformance.load_detail = lambda: {"summary": {"passed": 9, "total": 10}}
            query_conformance.conformance_summary_from_readme = lambda: {
                "passed": 10,
                "runnable": 10,
            }
            sys.argv = ["query-conformance.py", "--require-current-readme"]
            stderr = io.StringIO()
            with contextlib.redirect_stderr(stderr):
                exit_code = query_conformance.main()
        finally:
            query_conformance.load_detail = old_load_detail
            query_conformance.conformance_summary_from_readme = old_summary_from_readme
            sys.argv = old_argv

        self.assertEqual(exit_code, 1)
        self.assertIn("Conformance detail freshness: stale", stderr.getvalue())

    def test_require_current_readme_exits_zero_when_matched(self):
        old_load_detail = query_conformance.load_detail
        old_summary_from_readme = query_conformance.conformance_summary_from_readme
        old_argv = sys.argv
        try:
            query_conformance.load_detail = lambda: {"summary": {"passed": 10, "total": 10}}
            query_conformance.conformance_summary_from_readme = lambda: {
                "passed": 10,
                "runnable": 10,
            }
            sys.argv = ["query-conformance.py", "--require-current-readme"]
            stderr = io.StringIO()
            with contextlib.redirect_stderr(stderr):
                exit_code = query_conformance.main()
        finally:
            query_conformance.load_detail = old_load_detail
            query_conformance.conformance_summary_from_readme = old_summary_from_readme
            sys.argv = old_argv

        self.assertEqual(exit_code, 0)
        self.assertIn("aggregate-match", stderr.getvalue())

    def test_committed_conformance_claim_is_not_ahead_of_detail(self):
        """Keep any current public claim behind the committed detail artifact."""
        detail_summary = query_conformance.conformance_detail_summary(
            query_conformance.load_detail()
        )
        readme_text = query_conformance.README_FILE.read_text(encoding="utf-8")
        public_summary = query_conformance.conformance_summary_from_readme()
        if "<!-- CONFORMANCE_START -->" not in readme_text:
            self.assertNotIn(
                "<!-- CONFORMANCE_END -->",
                readme_text,
                "README contains an unmatched conformance aggregate marker",
            )
            self.assertIn(
                "## Frozen legacy checkpoint",
                readme_text,
                "README must either publish a checked current aggregate or identify "
                "its conformance numbers as frozen legacy evidence",
            )
            return
        self.assertIsNotNone(
            public_summary,
            "README conformance aggregate block is malformed; cannot evaluate freshness",
        )
        status = query_conformance.conformance_readme_freshness_status(
            detail_summary, public_summary
        )
        self.assertIn(
            status["state"],
            ("aggregate-match", "detail-ahead"),
            "committed scripts/conformance/conformance-detail.json is "
            f"'{status['state']}' relative to the README conformance aggregate "
            f"({status}); refresh conformance-detail.json or README.md's "
            "CONFORMANCE block before landing conformance metric claims.",
        )

if __name__ == "__main__":
    unittest.main()
