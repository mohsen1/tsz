#!/usr/bin/env python3
"""Render architecture report artifacts from arch_guard JSON output."""

from __future__ import annotations

import json
from pathlib import Path
from datetime import datetime, timezone


ROOT = Path(__file__).resolve().parents[2]
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


PREVIOUS_JSON = REPORT_DIR / "arch_guard_report.prev.json"


def load_previous_report() -> dict | None:
    if not PREVIOUS_JSON.exists():
        return None
    try:
        return json.loads(PREVIOUS_JSON.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError):
        return None


def build_failure_index(payload: dict) -> dict[str, int]:
    """Map failure group name -> hit count."""
    index: dict[str, int] = {}
    for failure in payload.get("failures", []):
        name = failure.get("name", "unnamed")
        index[name] = len(failure.get("hits", []))
    return index


def render_ratchet_section(current: dict, previous: dict | None) -> list[str]:
    """Render a ratchet metrics section comparing current vs previous runs."""
    lines: list[str] = []
    lines.append("## Ratchet Metrics")
    lines.append("")

    cur_total = current.get("total_hits", 0)
    cur_groups = len(current.get("failures", []))
    cur_index = build_failure_index(current)

    if previous is None:
        lines.append("*No previous report found for comparison.*")
        lines.append("")
        lines.append(f"- Total guard hits: **{cur_total}**")
        lines.append(f"- Failure groups: **{cur_groups}**")
        lines.append("")
        if cur_index:
            lines.append("| Guard rule | Hits | Status |")
            lines.append("|---|---:|---|")
            for name, count in sorted(cur_index.items()):
                lines.append(f"| {name} | {count} | FAIL |")
            lines.append("")
        return lines

    prev_total = previous.get("total_hits", 0)
    prev_groups = len(previous.get("failures", []))
    prev_index = build_failure_index(previous)

    delta_total = cur_total - prev_total
    delta_groups = cur_groups - prev_groups
    delta_sign = lambda d: f"+{d}" if d > 0 else str(d)

    lines.append(f"| Metric | Previous | Current | Delta |")
    lines.append("|---|---:|---:|---:|")
    lines.append(f"| Total guard hits | {prev_total} | {cur_total} | {delta_sign(delta_total)} |")
    lines.append(f"| Failure groups | {prev_groups} | {cur_groups} | {delta_sign(delta_groups)} |")
    lines.append("")

    all_names = sorted(set(cur_index.keys()) | set(prev_index.keys()))
    if all_names:
        lines.append("### Per-rule ratchet")
        lines.append("")
        lines.append("| Guard rule | Previous | Current | Delta | Status |")
        lines.append("|---|---:|---:|---:|---|")
        for name in all_names:
            prev_count = prev_index.get(name, 0)
            cur_count = cur_index.get(name, 0)
            delta = cur_count - prev_count
            if cur_count == 0 and prev_count > 0:
                status = "FIXED"
            elif cur_count == 0:
                status = "PASS"
            elif delta > 0:
                status = "REGRESSED"
            elif delta < 0:
                status = "IMPROVED"
            else:
                status = "UNCHANGED"
            lines.append(
                f"| {name} | {prev_count} | {cur_count} | {delta_sign(delta)} | {status} |"
            )
        lines.append("")

    return lines


def render_markdown(payload: dict) -> str:
    status = payload.get("status", "unknown")
    total_hits = payload.get("total_hits", 0)
    failures = payload.get("failures", [])
    previous = load_previous_report()
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

    lines.extend(render_ratchet_section(payload, previous))

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
    # Rotate current report to previous for next ratchet comparison
    import shutil
    shutil.copy2(INPUT_JSON, PREVIOUS_JSON)
    print(f"Architecture markdown report: {OUTPUT_MD}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
