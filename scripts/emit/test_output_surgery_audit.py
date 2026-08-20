import importlib.util
import io
import json
import pathlib
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from unittest.mock import patch


ROOT = pathlib.Path(__file__).resolve().parents[2]
AUDIT_PATH = ROOT / "scripts" / "emit" / "audit-output-surgery.py"


def load_audit_module():
    spec = importlib.util.spec_from_file_location("audit_output_surgery", AUDIT_PATH)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class OutputSurgeryAuditTests(unittest.TestCase):
    def setUp(self):
        self.audit = load_audit_module()

    def test_string_escaping_is_auto_allowed(self):
        line = "let escaped = s.replace('\\\\', \"\\\\\\\\\").replace('\"', \"\\\\\\\"\");"
        self.assertTrue(
            self.audit.is_auto_allowed_data_cleanup(
                "crates/tsz-core/src/emit.rs", line
            )
        )

    def test_semantic_output_rewrite_is_tracked(self):
        line = "output = output.replacen(&from, &to, 1);"
        self.assertFalse(
            self.audit.is_auto_allowed_data_cleanup(
                "crates/tsz-core/src/emit.rs", line
            )
        )

    def test_default_scan_targets_the_replacement_emitter(self):
        self.assertEqual(
            list(self.audit.iter_rust_files()),
            [ROOT / "crates" / "tsz-core" / "src" / "emit.rs"],
        )

    def test_missing_emitter_source_fails_closed(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            missing = pathlib.Path(temp_dir) / "missing.rs"
            with self.assertRaisesRegex(
                FileNotFoundError, "emitter source path does not exist"
            ):
                list(self.audit.iter_rust_files(missing))

    def test_manual_debt_marker_is_tracked(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            base = pathlib.Path(temp_dir)
            src = base / "demo.rs"
            src.write_text(
                "// OUTPUT_SURGERY_DEBT: manual DTS line rebuild\n",
                encoding="utf-8",
            )
            findings = self.audit.scan(base)

        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0].call, "manual")

    def test_allowlist_ratchets_counts(self):
        findings = [
            self.audit.Finding("a.rs", 1, "replacen", "output = output.replacen(&a, &b, 1);"),
            self.audit.Finding("a.rs", 2, "replace_range", "output.replace_range(0..1, x);"),
        ]
        failures = self.audit.audit(
            findings,
            {"a.rs": self.audit.AllowEntry("semantic-output-surgery", 1, "existing debt")},
        )
        self.assertEqual(failures, ["a.rs: 2 output-surgery call(s), allowlist max is 1"])

    def test_failure_summary_preserves_call_counts(self):
        summary = self.audit.summarize_failures(
            [
                "a.rs: 3 unallowlisted output-surgery call(s)",
                "b.rs: 3 output-surgery call(s), allowlist max is 2",
                "c.rs: allowlist entry is stale; no matching calls remain",
            ]
        )
        self.assertEqual(summary.unallowlisted, 3)
        self.assertEqual(summary.unallowlisted_files, 1)
        self.assertEqual(summary.over_allowlist, 1)
        self.assertEqual(summary.over_allowlist_files, 1)
        self.assertEqual(summary.over_allowlist_excess_calls, 1)
        self.assertEqual(summary.stale_allowlist, 1)
        self.assertEqual(summary.stale_allowlist_files, 1)

    def test_json_report_includes_summary_and_categories(self):
        findings = [
            self.audit.Finding("a.rs", 1, "replacen", "output = output.replacen(&a, &b, 1);"),
            self.audit.Finding("b.rs", 2, "replace", "rewritten = rewritten.replace(&a, &b);"),
        ]
        allowlist = {
            "b.rs": self.audit.AllowEntry("semantic-output-surgery", 1, "existing debt"),
            "c.rs": self.audit.AllowEntry("semantic-output-surgery", 1, "stale debt"),
        }
        failures = ["a.rs: 1 unallowlisted output-surgery call(s)"]
        git_context = {
            "repo_root": "/repo",
            "head": "abc123",
            "branch": "codex/studio-f-output-surgery",
            "upstream": "origin/main",
            "dirty": False,
            "dirty_path_count": 0,
        }

        report = self.audit.build_json_report(
            findings,
            allowlist,
            failures,
            git_context=git_context,
        )

        self.assertFalse(report["ok"])
        self.assertEqual(report["status"], "failed")
        self.assertEqual(report["output_surgery_status"], "failed")
        self.assertEqual(report["warning_count"], 0)
        self.assertEqual(report["warning_status"], "clear")
        self.assertEqual(report["git_context"], git_context)
        self.assertEqual(report["total_findings"], 2)
        self.assertEqual(report["files_with_findings"], 2)
        self.assertEqual(report["allowlisted_calls"], 1)
        self.assertEqual(report["allowlist_cap"], 2)
        self.assertEqual(report["remaining_allowlist_capacity"], 1)
        self.assertEqual(report["allowlist_budget_status"], "available")
        self.assertEqual(report["allowlist_pressure_status"], "available")
        self.assertEqual(report["exhausted_category_count"], 0)
        self.assertEqual(report["exhausted_categories"], [])
        self.assertEqual(report["unallowlisted_calls"], 1)
        self.assertEqual(report["over_allowlist_files"], 0)
        self.assertEqual(report["over_allowlist_excess_calls"], 0)
        self.assertEqual(report["stale_allowlist_files"], 0)
        self.assertEqual(report["failure_summary"]["unallowlisted"], 1)
        self.assertEqual(
            report["budget_summary"],
            {
                "allowlisted_calls": 1,
                "allowlist_cap": 2,
                "remaining_allowlist_capacity": 1,
                "allowlisted_files": 2,
                "budget_status": "available",
            },
        )
        self.assertEqual(
            report["categories"],
            [
                {
                    "category": "UNALLOWLISTED",
                    "count": 1,
                    "max_count": None,
                    "remaining_capacity": None,
                    "budget_status": "unallowlisted",
                    "files": 1,
                    "statuses": {"unallowlisted": 1},
                },
                {
                    "category": "semantic-output-surgery",
                    "count": 1,
                    "max_count": 2,
                    "remaining_capacity": 1,
                    "budget_status": "available",
                    "files": 2,
                    "statuses": {"allowlisted": 1, "stale_allowlist": 1},
                },
            ],
        )
        self.assertEqual(report["findings"][0]["category"], "UNALLOWLISTED")
        self.assertEqual(report["findings"][1]["category"], "semantic-output-surgery")
        self.assertEqual(
            [(entry["path"], entry["status"]) for entry in report["files"]],
            [
                ("a.rs", "unallowlisted"),
                ("b.rs", "allowlisted"),
                ("c.rs", "stale_allowlist"),
            ],
        )

    def test_json_report_records_ok_for_clean_audit(self):
        findings = [
            self.audit.Finding("a.rs", 1, "replacen", "output = output.replacen(&a, &b, 1);"),
        ]
        allowlist = {
            "a.rs": self.audit.AllowEntry("semantic-output-surgery", 1, "existing debt"),
        }

        report = self.audit.build_json_report(findings, allowlist, [])

        self.assertTrue(report["ok"])
        self.assertEqual(report["status"], "passed")
        self.assertEqual(report["output_surgery_status"], "passed")
        self.assertEqual(report["warning_count"], 1)
        self.assertEqual(report["warning_status"], "warn")
        self.assertEqual(report["allowlist_pressure_status"], "blocked")
        self.assertEqual(report["exhausted_category_count"], 1)
        self.assertEqual(report["exhausted_categories"], ["semantic-output-surgery"])
        self.assertEqual(report["failures"], [])

    def test_git_context_records_branch_upstream_and_dirty_count(self):
        calls = []

        def fake_run_git(root, args):
            calls.append((root, tuple(args)))
            responses = {
                ("status", "--porcelain"): " M scripts/emit/audit-output-surgery.py\n?? tmp.txt",
                ("branch", "--show-current"): "codex/studio-f-output-surgery",
                ("rev-parse", "HEAD"): "abc123",
                (
                    "rev-parse",
                    "--abbrev-ref",
                    "--symbolic-full-name",
                    "@{u}",
                ): "origin/main",
            }
            return responses.get(tuple(args))

        root = pathlib.Path("/repo")
        context = self.audit.build_git_context(root, run_git=fake_run_git)

        self.assertEqual(context["repo_root"], "/repo")
        self.assertEqual(context["head"], "abc123")
        self.assertEqual(context["branch"], "codex/studio-f-output-surgery")
        self.assertEqual(context["upstream"], "origin/main")
        self.assertTrue(context["dirty"])
        self.assertEqual(context["dirty_path_count"], 2)
        self.assertIn((root, ("status", "--porcelain")), calls)

    def test_pass_summary_names_clean_guardrail_counters(self):
        findings = [
            self.audit.Finding("a.rs", 1, "replacen", "output = output.replacen(&a, &b, 1);"),
            self.audit.Finding("b.rs", 2, "replace", "rewritten = rewritten.replace(&a, &b);"),
        ]
        allowlist = {
            "a.rs": self.audit.AllowEntry("semantic-output-surgery", 1, "existing debt"),
            "b.rs": self.audit.AllowEntry("semantic-output-surgery", 1, "existing debt"),
        }

        summary = self.audit.format_pass_summary(findings, [], allowlist)

        self.assertEqual(
            summary,
            "Output-surgery audit passed: "
            "total_findings=2, "
            "files_with_findings=2, "
            "allowlisted_calls=2, "
            "allowlist_cap=2, "
            "remaining_allowlist_capacity=0, "
            "allowlist_budget_status=exhausted, "
            "category_budgets=semantic-output-surgery=2/2:exhausted, "
            "exhausted_category_count=1, "
            "exhausted_categories=semantic-output-surgery, "
            "allowlist_pressure_status=blocked, "
            "warning_count=1, "
            "warning_status=warn, "
            "unallowlisted_calls=0, "
            "over_allowlist_files=0, "
            "over_allowlist_excess_calls=0, "
            "stale_allowlist_files=0.",
        )

    def test_category_budget_metrics_names_each_category(self):
        file_summaries = [
            {
                "path": "a.rs",
                "count": 2,
                "category": "dts-output-surgery",
                "max_count": 3,
                "reason": "existing debt",
                "status": "allowlisted",
            },
            {
                "path": "b.rs",
                "count": 1,
                "category": "ir-output-surgery",
                "max_count": 1,
                "reason": "existing debt",
                "status": "allowlisted",
            },
            {
                "path": "c.rs",
                "count": 4,
                "category": "UNALLOWLISTED",
                "max_count": None,
                "reason": None,
                "status": "unallowlisted",
            },
        ]

        summary = self.audit.format_category_budget_metrics(file_summaries)

        self.assertEqual(
            summary,
            "category_budgets=UNALLOWLISTED=unallowlisted:4,"
            "dts-output-surgery=2/3:available,"
            "ir-output-surgery=1/1:exhausted",
        )

    def test_exhausted_category_metrics_name_only_zero_capacity_categories(self):
        file_summaries = [
            {
                "path": "a.rs",
                "count": 2,
                "category": "dts-output-surgery",
                "max_count": 3,
                "reason": "existing debt",
                "status": "allowlisted",
            },
            {
                "path": "b.rs",
                "count": 1,
                "category": "ir-output-surgery",
                "max_count": 1,
                "reason": "existing debt",
                "status": "allowlisted",
            },
            {
                "path": "c.rs",
                "count": 4,
                "category": "UNALLOWLISTED",
                "max_count": None,
                "reason": None,
                "status": "unallowlisted",
            },
        ]

        self.assertEqual(self.audit.exhausted_category_names(file_summaries), ["ir-output-surgery"])
        self.assertEqual(
            self.audit.format_exhausted_category_metrics(file_summaries),
            "exhausted_category_count=1, exhausted_categories=ir-output-surgery",
        )
        self.assertEqual(
            self.audit.format_warning_metrics(file_summaries),
            "warning_count=1, warning_status=warn",
        )

    def test_budget_status_classifies_budget_edges(self):
        self.assertEqual(self.audit.classify_budget_status(0, 0), "no_allowlist")
        self.assertEqual(self.audit.classify_budget_status(1, 2), "available")
        self.assertEqual(self.audit.classify_budget_status(2, 2), "exhausted")
        self.assertEqual(self.audit.classify_budget_status(3, 2), "over_cap")

    def test_allowlist_pressure_distinguishes_global_and_category_blocks(self):
        budget = self.audit.BudgetSummary(
            allowlisted_calls=3,
            allowlist_cap=4,
            remaining_allowlist_capacity=1,
            allowlisted_files=2,
            budget_status="available",
        )
        self.assertEqual(
            self.audit.classify_allowlist_pressure(budget, ["dts-output-surgery"]),
            "category_blocked",
        )

        budget = self.audit.BudgetSummary(
            allowlisted_calls=4,
            allowlist_cap=4,
            remaining_allowlist_capacity=0,
            allowlisted_files=2,
            budget_status="exhausted",
        )
        self.assertEqual(
            self.audit.classify_allowlist_pressure(budget, ["dts-output-surgery"]),
            "blocked",
        )

    def test_warning_failures_report_pressure_for_strict_mode(self):
        file_summaries = [
            {
                "path": "a.rs",
                "count": 1,
                "category": "dts-output-surgery",
                "max_count": 1,
                "reason": "existing debt",
                "status": "allowlisted",
            }
        ]

        self.assertEqual(
            self.audit.warning_failures(file_summaries),
            [
                "allowlist pressure is blocked; exhausted categories: "
                "dts-output-surgery"
            ],
        )

    def test_warning_failures_allow_available_budget(self):
        file_summaries = [
            {
                "path": "a.rs",
                "count": 1,
                "category": "dts-output-surgery",
                "max_count": 2,
                "reason": "existing debt",
                "status": "allowlisted",
            }
        ]

        self.assertEqual(self.audit.warning_failures(file_summaries), [])

    def test_fail_on_warnings_returns_distinct_status(self):
        findings = [
            self.audit.Finding("a.rs", 1, "replacen", "output = output.replacen(&a, &b, 1);"),
        ]
        allowlist = {
            "a.rs": self.audit.AllowEntry("dts-output-surgery", 1, "existing debt"),
        }

        with (
            patch.object(self.audit, "scan", return_value=findings),
            patch.object(self.audit, "load_allowlist", return_value=allowlist),
            redirect_stdout(io.StringIO()) as stdout,
            redirect_stderr(io.StringIO()) as stderr,
        ):
            status = self.audit.main(["--fail-on-warnings"])

        self.assertEqual(status, 2)
        self.assertIn("allowlist_pressure_status=blocked", stdout.getvalue())
        self.assertIn("Output-surgery audit warnings failed", stderr.getvalue())

    def test_json_fail_on_warnings_reports_strict_warning_failure(self):
        findings = [
            self.audit.Finding("a.rs", 1, "replacen", "output = output.replacen(&a, &b, 1);"),
        ]
        allowlist = {
            "a.rs": self.audit.AllowEntry("dts-output-surgery", 1, "existing debt"),
        }

        with (
            patch.object(self.audit, "scan", return_value=findings),
            patch.object(self.audit, "load_allowlist", return_value=allowlist),
            patch.object(self.audit, "build_git_context", return_value={"head": "abc123"}),
            redirect_stdout(io.StringIO()) as stdout,
            redirect_stderr(io.StringIO()) as stderr,
        ):
            status = self.audit.main(["--json", "--fail-on-warnings"])

        payload = json.loads(stdout.getvalue())
        self.assertEqual(status, 2)
        self.assertFalse(payload["ok"])
        self.assertEqual(payload["status"], "warning_failed")
        self.assertEqual(payload["output_surgery_status"], "passed")
        self.assertTrue(payload["fail_on_warnings"])
        self.assertEqual(payload["strict_warning_status"], "failed")
        self.assertEqual(payload["warning_failure_count"], 1)
        self.assertEqual(
            payload["warning_failures"],
            [
                "allowlist pressure is blocked; exhausted categories: "
                "dts-output-surgery"
            ],
        )
        self.assertIn("Output-surgery audit warnings failed", stderr.getvalue())

    def test_default_mode_allows_warnings(self):
        findings = [
            self.audit.Finding("a.rs", 1, "replacen", "output = output.replacen(&a, &b, 1);"),
        ]
        allowlist = {
            "a.rs": self.audit.AllowEntry("dts-output-surgery", 1, "existing debt"),
        }

        with (
            patch.object(self.audit, "scan", return_value=findings),
            patch.object(self.audit, "load_allowlist", return_value=allowlist),
            redirect_stdout(io.StringIO()) as stdout,
            redirect_stderr(io.StringIO()) as stderr,
        ):
            status = self.audit.main([])

        self.assertEqual(status, 0)
        self.assertIn("warning_status=warn", stdout.getvalue())
        self.assertEqual(stderr.getvalue(), "")

    def test_json_mode_writes_stdout_report_without_text_summary(self):
        findings = [
            self.audit.Finding("a.rs", 1, "replacen", "output = output.replacen(&a, &b, 1);"),
        ]
        allowlist = {
            "a.rs": self.audit.AllowEntry("dts-output-surgery", 2, "existing debt"),
        }

        with (
            patch.object(self.audit, "scan", return_value=findings),
            patch.object(self.audit, "load_allowlist", return_value=allowlist),
            patch.object(self.audit, "build_git_context", return_value={"head": "abc123"}),
            redirect_stdout(io.StringIO()) as stdout,
            redirect_stderr(io.StringIO()) as stderr,
        ):
            status = self.audit.main(["--json"])

        payload = json.loads(stdout.getvalue())
        self.assertEqual(status, 0)
        self.assertTrue(payload["ok"])
        self.assertEqual(payload["git_context"], {"head": "abc123"})
        self.assertEqual(payload["allowlisted_calls"], 1)
        self.assertNotIn("Output-surgery audit passed", stdout.getvalue())
        self.assertEqual(stderr.getvalue(), "")

    def test_json_mode_preserves_failure_exit_and_stderr_summary(self):
        findings = [
            self.audit.Finding("a.rs", 1, "replacen", "output = output.replacen(&a, &b, 1);"),
        ]

        with (
            patch.object(self.audit, "scan", return_value=findings),
            patch.object(self.audit, "load_allowlist", return_value={}),
            patch.object(self.audit, "build_git_context", return_value={"head": "abc123"}),
            redirect_stdout(io.StringIO()) as stdout,
            redirect_stderr(io.StringIO()) as stderr,
        ):
            status = self.audit.main(["--json"])

        payload = json.loads(stdout.getvalue())
        self.assertEqual(status, 1)
        self.assertFalse(payload["ok"])
        self.assertEqual(payload["unallowlisted_calls"], 1)
        self.assertNotIn("a.rs [UNALLOWLISTED]", stdout.getvalue())
        self.assertIn("Output-surgery audit failed", stderr.getvalue())

    def test_write_json_report_creates_parent_and_writes_json(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            report_path = pathlib.Path(temp_dir) / "nested" / "report.json"
            self.audit.write_json_report(report_path, {"ok": True, "value": 42})
            payload = json.loads(report_path.read_text(encoding="utf-8"))

        self.assertEqual(payload, {"ok": True, "value": 42})

    def test_scan_ignores_data_cleanup_but_finds_output_surgery(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            base = pathlib.Path(temp_dir)
            src = base / "demo.rs"
            src.write_text(
                "\n".join(
                    [
                        "let escaped = s.replace('\\\\', \"\\\\\\\\\");",
                        "// OUTPUT_SURGERY_DEBT: manual DTS line rebuild",
                        "output = output.replacen(&from, &to, 1);",
                    ]
                ),
                encoding="utf-8",
            )
            findings = self.audit.scan(base)
        self.assertEqual(len(findings), 2)
        self.assertEqual(findings[0].line_no, 2)
        self.assertEqual(findings[1].line_no, 3)


if __name__ == "__main__":
    unittest.main()
