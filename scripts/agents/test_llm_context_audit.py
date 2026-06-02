import importlib.util
import json
import pathlib
import tempfile
import unittest
from unittest import mock


ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "agents" / "llm-context-audit.py"
SPEC = importlib.util.spec_from_file_location("llm_context_audit", SCRIPT)
llm_context_audit = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(llm_context_audit)


class LlmContextAuditTests(unittest.TestCase):
    def test_current_repo_passes(self):
        findings, metrics = llm_context_audit.audit()

        self.assertEqual([], findings)
        self.assertTrue(metrics["same_instruction_target"])
        self.assertTrue(metrics["agents_is_symlink"])
        self.assertGreater(metrics["instruction_lines"], 0)
        self.assertLessEqual(metrics["instruction_lines"], 260)
        self.assertLessEqual(metrics["instruction_bytes"], 20_000)
        self.assertLessEqual(metrics["max_skill_lines"], 120)
        self.assertLessEqual(metrics["max_skill_words"], 900)

    def test_forbidden_hook_fragments_are_reported(self):
        with mock.patch.object(
            llm_context_audit,
            "load_json",
            side_effect=lambda path: {
                "hooks": {
                    "SessionStart": [
                        {
                            "hooks": [
                                {
                                    "type": "command",
                                    "command": 'git -C "$root" pull --rebase origin main; cat "$root/AGENTS.md"',
                                }
                            ]
                        }
                    ]
                },
                "env": {},
            },
        ):
            findings, _metrics = llm_context_audit.audit()

        self.assertTrue(
            any("cat \"$root/AGENTS.md\"" in finding for finding in findings),
            findings,
        )
        self.assertTrue(
            any("git -C \"$root\" pull --rebase" in finding for finding in findings),
            findings,
        )

    def test_forced_large_output_env_is_reported(self):
        def fake_load_json(path):
            if path.name == "settings.json":
                return {"hooks": {}, "env": {"CLAUDE_CODE_MAX_OUTPUT_TOKENS": "64000"}}
            return {"hooks": {}}

        with mock.patch.object(llm_context_audit, "load_json", side_effect=fake_load_json):
            with mock.patch.object(
                llm_context_audit,
                "parse_simple_toml_set",
                return_value={"CLAUDE_CODE_MAX_OUTPUT_TOKENS": "64000"},
            ):
                findings, _metrics = llm_context_audit.audit()

        self.assertTrue(
            any(
                ".claude/settings.json sets CLAUDE_CODE_MAX_OUTPUT_TOKENS" in finding
                for finding in findings
            ),
            findings,
        )
        self.assertTrue(
            any(
                ".codex/config.toml sets CLAUDE_CODE_MAX_OUTPUT_TOKENS" in finding
                for finding in findings
            ),
            findings,
        )

    def test_instruction_budget_is_reported(self):
        original_read_text = pathlib.Path.read_text

        def fake_read_text(path, *args, **kwargs):
            if pathlib.Path(path).name == "CLAUDE.md":
                return "\n".join(f"line {i}" for i in range(300))
            return original_read_text(path, *args, **kwargs)

        with mock.patch.object(pathlib.Path, "read_text", fake_read_text):
            findings, _metrics = llm_context_audit.audit()

        self.assertTrue(
            any("exceeds line budget" in finding for finding in findings),
            findings,
        )

    def test_skill_budget_is_reported(self):
        original_glob = pathlib.Path.glob
        original_read_text = pathlib.Path.read_text
        fake_skill = ROOT / ".agents" / "skills" / "fake" / "SKILL.md"

        def fake_glob(path, pattern):
            if str(path) == str(ROOT) and pattern == ".agents/skills/*/SKILL.md":
                return [fake_skill]
            if str(path) == str(ROOT) and pattern == ".claude/skills/*/SKILL.md":
                return []
            return original_glob(path, pattern)

        def fake_read_text(path, *args, **kwargs):
            if pathlib.Path(path) == fake_skill:
                return "\n".join("word" for _ in range(130))
            return original_read_text(path, *args, **kwargs)

        with mock.patch.object(pathlib.Path, "glob", fake_glob):
            with mock.patch.object(pathlib.Path, "read_text", fake_read_text):
                findings, _metrics = llm_context_audit.audit()

        self.assertTrue(
            any("SKILL.md exceeds skill line budget" in finding for finding in findings),
            findings,
        )

    def test_json_report_shape(self):
        with tempfile.TemporaryDirectory(dir=ROOT) as temp_dir:
            report_path = pathlib.Path(temp_dir) / "reports" / "llm-context.json"
            with mock.patch.object(
                llm_context_audit.sys,
                "argv",
                ["llm-context-audit.py", "--json-report", str(report_path)],
            ):
                exit_code = llm_context_audit.main()

            report = json.loads(report_path.read_text(encoding="utf-8"))

        self.assertEqual(0, exit_code)
        self.assertTrue(report["ok"])
        self.assertEqual("pass", report["status"])
        self.assertEqual("scripts/agents/llm-context-audit.py", report["generated_by"])
        self.assertEqual([], report["findings"])


if __name__ == "__main__":
    unittest.main()
