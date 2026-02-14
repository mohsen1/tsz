#!/usr/bin/env python3
"""Render architecture report artifacts from arch_guard JSON output."""

from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
REPORT_DIR = ROOT / "artifacts" / "architecture"
INPUT_JSON = REPORT_DIR / "arch_guard_report.json"
OUTPUT_MD = REPORT_DIR / "arch_guard_report.md"


def collect_largest_rs_files(limit: int = 10) -> list[tuple[str, int]]:
    rows: list[tuple[str, int]] = []
    for path in ROOT.rglob("*.rs"):
        parts = set(path.parts)
        if "target" in parts or ".git" in parts:
            continue
        try:
            line_count = sum(1 for _ in path.open("r", encoding="utf-8", errors="ignore"))
        except OSError:
            continue
        rel = path.relative_to(ROOT).as_posix()
        rows.append((rel, line_count))
    rows.sort(key=lambda item: item[1], reverse=True)
    return rows[:limit]


def render_markdown(payload: dict) -> str:
    status = payload.get("status", "unknown")
    total_hits = payload.get("total_hits", 0)
    failures = payload.get("failures", [])
    largest_files = collect_largest_rs_files()

    lines: list[str] = []
    lines.append("# Architecture Guard Report")
    lines.append("")
    lines.append(f"- Status: **{status}**")
    lines.append(f"- Total guard hits: **{total_hits}**")
    lines.append(f"- Failure groups: **{len(failures)}**")
    lines.append("")

    lines.append("## Largest Rust files")
    lines.append("")
    lines.append("| File | Lines |")
    lines.append("|---|---:|")
    for rel, line_count in largest_files:
        lines.append(f"| `{rel}` | {line_count} |")
    lines.append("")

    lines.append("## Guard failures")
    lines.append("")
    if not failures:
        lines.append("- None")
    else:
        for failure in failures:
            name = failure.get("name", "unnamed")
            hits = failure.get("hits", [])
            lines.append(f"- **{name}** ({len(hits)} hits)")
            for hit in hits[:10]:
                lines.append(f"  - `{hit}`")
            if len(hits) > 10:
                lines.append(f"  - ... and {len(hits) - 10} more")
    lines.append("")
    return "\n".join(lines)


def main() -> int:
    if not INPUT_JSON.exists():
        raise SystemExit(f"missing report input: {INPUT_JSON}")
    payload = json.loads(INPUT_JSON.read_text(encoding="utf-8"))
    OUTPUT_MD.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT_MD.write_text(render_markdown(payload), encoding="utf-8")
    print(f"Architecture markdown report: {OUTPUT_MD}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
