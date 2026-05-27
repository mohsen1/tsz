import importlib.util
import json
import pathlib
import sys
import tempfile
import unittest


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
                "crates/tsz-emitter/src/enums/transform.rs", line
            )
        )

    def test_semantic_output_rewrite_is_tracked(self):
        line = "output = output.replacen(&from, &to, 1);"
        self.assertFalse(
            self.audit.is_auto_allowed_data_cleanup(
                "crates/tsz-emitter/src/emitter/transform_dispatch.rs", line
            )
        )

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

        report = self.audit.build_json_report(findings, allowlist, failures)

        self.assertFalse(report["ok"])
        self.assertEqual(report["total_findings"], 2)
        self.assertEqual(report["files_with_findings"], 2)
        self.assertEqual(report["failure_summary"]["unallowlisted"], 1)
        self.assertEqual(
            report["categories"],
            [
                {
                    "category": "UNALLOWLISTED",
                    "count": 1,
                    "max_count": None,
                    "files": 1,
                    "statuses": {"unallowlisted": 1},
                },
                {
                    "category": "semantic-output-surgery",
                    "count": 1,
                    "max_count": 2,
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

    def test_pass_summary_names_clean_guardrail_counters(self):
        findings = [
            self.audit.Finding("a.rs", 1, "replacen", "output = output.replacen(&a, &b, 1);"),
            self.audit.Finding("b.rs", 2, "replace", "rewritten = rewritten.replace(&a, &b);"),
        ]

        summary = self.audit.format_pass_summary(findings, [])

        self.assertEqual(
            summary,
            "Output-surgery audit passed: "
            "total_findings=2, "
            "files_with_findings=2, "
            "unallowlisted_calls=0, "
            "over_allowlist_files=0, "
            "over_allowlist_excess_calls=0, "
            "stale_allowlist_files=0.",
        )

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
                        "output = output.replacen(&from, &to, 1);",
                    ]
                ),
                encoding="utf-8",
            )
            findings = self.audit.scan(base)
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0].line_no, 2)


if __name__ == "__main__":
    unittest.main()
