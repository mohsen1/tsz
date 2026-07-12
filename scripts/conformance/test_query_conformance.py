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


if __name__ == "__main__":
    unittest.main()
