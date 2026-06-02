#!/usr/bin/env python3
"""Audit repo-local LLM coding context for avoidable startup token waste."""

from __future__ import annotations

import argparse
import json
import pathlib
import sys
from typing import Any


ROOT = pathlib.Path(__file__).resolve().parents[2]
FORBIDDEN_HOOK_FRAGMENTS = (
    "cat \"$root/AGENTS.md\"",
    "cat '$root/AGENTS.md'",
    "cat $root/AGENTS.md",
    "cat AGENTS.md",
    "git -C \"$root\" pull --rebase",
    "git pull --rebase",
)
FORBIDDEN_ENV_KEYS = {
    "CLAUDE_CODE_MAX_OUTPUT_TOKENS": (
        "Project defaults should not force large model outputs; Claude docs note "
        "that increasing this reduces effective context before auto-compaction."
    )
}


def load_json(path: pathlib.Path) -> Any:
    with path.open(encoding="utf-8") as f:
        return json.load(f)


def flatten_hook_commands(value: Any) -> list[str]:
    commands: list[str] = []
    if isinstance(value, dict):
        if value.get("type") == "command" and isinstance(value.get("command"), str):
            commands.append(value["command"])
        for child in value.values():
            commands.extend(flatten_hook_commands(child))
    elif isinstance(value, list):
        for child in value:
            commands.extend(flatten_hook_commands(child))
    return commands


def parse_simple_toml_set(path: pathlib.Path) -> dict[str, str]:
    in_set = False
    result: dict[str, str] = {}
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("[") and line.endswith("]"):
            in_set = line == "[shell_environment_policy.set]"
            continue
        if not in_set or "=" not in line:
            continue
        key, value = line.split("=", 1)
        result[key.strip()] = value.strip().strip('"')
    return result


def audit() -> tuple[list[str], dict[str, Any]]:
    findings: list[str] = []
    metrics: dict[str, Any] = {}

    agents_path = ROOT / "AGENTS.md"
    claude_path = ROOT / ".claude" / "CLAUDE.md"
    agents_real = agents_path.resolve()
    claude_real = claude_path.resolve()
    metrics["agents_realpath"] = str(agents_real.relative_to(ROOT))
    metrics["claude_realpath"] = str(claude_real.relative_to(ROOT))
    metrics["agents_is_symlink"] = agents_path.is_symlink()
    metrics["same_instruction_target"] = agents_real == claude_real
    if agents_real != claude_real:
        findings.append(
            "AGENTS.md and .claude/CLAUDE.md must resolve to one canonical "
            "instruction file"
        )

    instruction_text = agents_real.read_text(encoding="utf-8")
    metrics["instruction_lines"] = len(instruction_text.splitlines())
    metrics["instruction_bytes"] = len(instruction_text.encode("utf-8"))

    for rel_path in (".codex/hooks.json", ".claude/settings.json"):
        path = ROOT / rel_path
        data = load_json(path)
        commands = flatten_hook_commands(data.get("hooks", {}))
        metrics[f"{rel_path}:command_hooks"] = len(commands)
        for command in commands:
            for fragment in FORBIDDEN_HOOK_FRAGMENTS:
                if fragment in command:
                    findings.append(
                        f"{rel_path} hook reintroduces startup context/mutation "
                        f"fragment: {fragment}"
                    )

    claude_settings = load_json(ROOT / ".claude" / "settings.json")
    claude_env = claude_settings.get("env", {})
    if not isinstance(claude_env, dict):
        findings.append(".claude/settings.json env must be an object")
        claude_env = {}
    codex_env = parse_simple_toml_set(ROOT / ".codex" / "config.toml")
    for source, env in (
        (".claude/settings.json", claude_env),
        (".codex/config.toml", codex_env),
    ):
        for key, reason in FORBIDDEN_ENV_KEYS.items():
            if key in env:
                findings.append(f"{source} sets {key}: {reason}")

    metrics["finding_count"] = len(findings)
    return findings, metrics


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json-report", type=pathlib.Path)
    args = parser.parse_args()

    findings, metrics = audit()
    status = "fail" if findings else "pass"
    print(f"llm_context_audit_status={status}")
    for key in sorted(metrics):
        print(f"{key}={metrics[key]}")
    for finding in findings:
        print(f"finding={finding}")

    if args.json_report:
        args.json_report.parent.mkdir(parents=True, exist_ok=True)
        args.json_report.write_text(
            json.dumps(
                {
                    "ok": not findings,
                    "status": status,
                    "generated_by": "scripts/agents/llm-context-audit.py",
                    "metrics": metrics,
                    "findings": findings,
                },
                indent=2,
                sort_keys=True,
            )
            + "\n",
            encoding="utf-8",
        )

    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
