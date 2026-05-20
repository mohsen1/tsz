import importlib.util
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
