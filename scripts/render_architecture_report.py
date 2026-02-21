#!/usr/bin/env python3
"""Render architecture report artifacts from arch_guard JSON output."""

from __future__ import annotations

import json
from pathlib import Path
from datetime import datetime, timezone


ROOT = Path(__file__).resolve().parents[1]
REPORT_DIR = ROOT / "artifacts" / "architecture"
INPUT_JSON = REPORT_DIR / "arch_guard_report.json"
OUTPUT_MD = REPORT_DIR / "arch_guard_report.md"


def count_lines(path: Path) -> int | None:
    try:
        with path.open("r", encoding="utf-8", errors="ignore") as handle:
            return sum(1 for _ in handle)
    except OSError:
        return None


def collect_largest_rs_files(limit: int = 10, *, include_tests: bool = True) -> list[tuple[str, int]]:
    rows: list[tuple[str, int]] = []
    for path in ROOT.rglob("*.rs"):
        parts = set(path.parts)
        if "target" in parts or ".git" in parts:
            continue
        if not include_tests and "tests" in parts:
            continue
        line_count = count_lines(path)
        if line_count is None:
            continue
        rel = path.relative_to(ROOT).as_posix()
        rows.append((rel, line_count))
    rows.sort(key=lambda item: item[1], reverse=True)
    return rows[:limit]


def collect_largest_by_crate(*, include_tests: bool = True) -> list[tuple[str, str, int]]:
    targets = {
        "tsz-checker": "crates/tsz-checker",
        "tsz-solver": "crates/tsz-solver",
        "tsz-lsp": "crates/tsz-lsp",
        "tsz-emitter": "crates/tsz-emitter",
    }
    rows: list[tuple[str, str, int]] = []
    for crate_name, crate_root in targets.items():
        root = ROOT / crate_root
        largest_file = ""
        largest_lines = -1
        for path in root.rglob("*.rs"):
            if not include_tests and "tests" in path.parts:
                continue
            line_count = count_lines(path)
            if line_count is None:
                continue
            if line_count > largest_lines:
                largest_lines = line_count
                largest_file = path.relative_to(ROOT).as_posix()
        if largest_file:
            rows.append((crate_name, largest_file, largest_lines))
    return rows


def render_markdown(payload: dict) -> str:
    status = payload.get("status", "unknown")
    total_hits = payload.get("total_hits", 0)
    failures = payload.get("failures", [])
    largest_files = collect_largest_rs_files()
    largest_source_files = collect_largest_rs_files(include_tests=False)
    largest_by_crate = collect_largest_by_crate()
    largest_source_by_crate = collect_largest_by_crate(include_tests=False)

    lines: list[str] = []
    lines.append("# Architecture Guard Report")
    lines.append("")
    generated_at = datetime.now(timezone.utc).isoformat()
    lines.append(f"- Generated at (UTC): `{generated_at}`")
    lines.append(f"- Source JSON: `{INPUT_JSON.relative_to(ROOT).as_posix()}`")
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

    lines.append("## Largest Rust source files (excluding tests)")
    lines.append("")
    lines.append("| File | Lines |")
    lines.append("|---|---:|")
    for rel, line_count in largest_source_files:
        lines.append(f"| `{rel}` | {line_count} |")
    lines.append("")

    lines.append("## Largest file per core crate")
    lines.append("")
    lines.append("| Crate | File | Lines |")
    lines.append("|---|---|---:|")
    for crate_name, rel, line_count in largest_by_crate:
        lines.append(f"| `{crate_name}` | `{rel}` | {line_count} |")
    lines.append("")

    lines.append("## Largest source file per core crate (excluding tests)")
    lines.append("")
    lines.append("| Crate | File | Lines |")
    lines.append("|---|---|---:|")
    for crate_name, rel, line_count in largest_source_by_crate:
        lines.append(f"| `{crate_name}` | `{rel}` | {line_count} |")
    lines.append("")

    lines.append("## Guard failures")
    lines.append("")
    if not failures:
        lines.append("- None")
    else:
        lines.append("| Failure group | Hits |")
        lines.append("|---|---:|")
        for failure in failures:
            name = failure.get("name", "unnamed")
            hits = failure.get("hits", [])
            lines.append(f"| {name} | {len(hits)} |")
        lines.append("")

        lines.append("### Failure details")
        lines.append("")
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
