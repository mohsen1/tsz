#!/usr/bin/env python3
"""Compare two scale-cliff JSON artifacts and emit review-friendly reports."""

from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


RATIO_FIELDS = (
    "ratio_checkers_per_file",
    "ratio_overlay_per_file",
    "ratio_delegations_per_file",
    "ratio_compute_per_file",
)


def load_artifact(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def rows_by_fixture(artifact: dict[str, Any]) -> dict[str, dict[str, Any]]:
    rows = artifact.get("rows") or []
    return {
        str(row.get("fixture")): row
        for row in rows
        if row.get("fixture") is not None
    }


def relative_delta(previous: float, current: float) -> float | None:
    if previous == 0:
        return None
    return round((current - previous) / previous, 6)


def classify_change(previous: float | None, current: float | None, threshold: float) -> str:
    if previous is None and current is None:
        return "stable"
    if previous is None:
        return "new"
    if current is None:
        return "missing"
    if previous == 0:
        delta = current - previous
    else:
        delta = (current - previous) / previous
    if delta >= threshold:
        return "regression"
    if delta <= -threshold:
        return "improvement"
    return "stable"


def numeric_value(row: dict[str, Any] | None, field: str) -> float | None:
    if row is None or row.get(field) is None:
        return None
    return float(row[field])


def build_report(
    previous: dict[str, Any],
    current: dict[str, Any],
    *,
    previous_path: Path,
    current_path: Path,
    generated_at: str,
    threshold: float,
) -> dict[str, Any]:
    previous_rows = rows_by_fixture(previous)
    current_rows = rows_by_fixture(current)
    fixtures = sorted(set(previous_rows) | set(current_rows))

    changes: list[dict[str, Any]] = []
    totals = {
        "fixtures_compared": len(fixtures),
        "ratio_fields": len(RATIO_FIELDS),
        "regressions": 0,
        "improvements": 0,
        "stable": 0,
        "new": 0,
        "missing": 0,
    }

    for fixture in fixtures:
        previous_row = previous_rows.get(fixture)
        current_row = current_rows.get(fixture)
        for field in RATIO_FIELDS:
            previous_value = numeric_value(previous_row, field)
            current_value = numeric_value(current_row, field)
            status = classify_change(previous_value, current_value, threshold)
            totals[status + "s" if status in {"regression", "improvement"} else status] += 1
            delta = None
            if previous_value is not None and current_value is not None:
                delta = round(current_value - previous_value, 6)
            changes.append(
                {
                    "fixture": fixture,
                    "field": field,
                    "previous": previous_value,
                    "current": current_value,
                    "delta": delta,
                    "relative_delta": (
                        None
                        if previous_value is None or current_value is None
                        else relative_delta(previous_value, current_value)
                    ),
                    "status": status,
                }
            )

    return {
        "schema_version": 1,
        "generated_at": generated_at,
        "previous_file": str(previous_path),
        "current_file": str(current_path),
        "threshold": threshold,
        "totals": totals,
        "changes": changes,
    }


def format_number(value: float | None) -> str:
    if value is None:
        return "-"
    return f"{value:.2f}"


def format_percent(value: float | None) -> str:
    if value is None:
        return "-"
    return f"{value * 100:.1f}%"


def markdown_report(report: dict[str, Any]) -> str:
    totals = report["totals"]
    notable = [
        change
        for change in report["changes"]
        if change["status"] in {"regression", "improvement", "new", "missing"}
    ]
    notable.sort(
        key=lambda change: (
            {"regression": 0, "improvement": 1, "new": 2, "missing": 3}.get(change["status"], 4),
            change["fixture"],
            change["field"],
        )
    )

    lines = [
        "# Scale-Cliff Ratio Comparison",
        "",
        f"Generated at: `{report['generated_at']}`",
        f"Previous: `{report['previous_file']}`",
        f"Current: `{report['current_file']}`",
        f"Regression threshold: `{report['threshold']:.2f}`",
        "",
        "| Metric | Count |",
        "| --- | ---: |",
        f"| Fixtures compared | {totals['fixtures_compared']} |",
        f"| Regressions | {totals['regressions']} |",
        f"| Improvements | {totals['improvements']} |",
        f"| Stable ratios | {totals['stable']} |",
        f"| New ratios | {totals['new']} |",
        f"| Missing ratios | {totals['missing']} |",
        "",
    ]

    if notable:
        lines.extend(
            [
                "## Notable Changes",
                "",
                "| Status | Fixture | Ratio | Previous | Current | Delta | Relative |",
                "| --- | --- | --- | ---: | ---: | ---: | ---: |",
            ]
        )
        for change in notable:
            lines.append(
                "| {status} | `{fixture}` | `{field}` | {previous} | {current} | {delta} | {relative} |".format(
                    status=change["status"],
                    fixture=change["fixture"],
                    field=change["field"],
                    previous=format_number(change["previous"]),
                    current=format_number(change["current"]),
                    delta=format_number(change["delta"]),
                    relative=format_percent(change["relative_delta"]),
                )
            )
    else:
        lines.append("No ratio changes crossed the configured threshold.")

    return "\n".join(lines) + "\n"


def write_json(path: Path, report: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")


def write_markdown(path: Path, report: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(markdown_report(report), encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("previous", type=Path, help="previous scale-cliff JSON artifact")
    parser.add_argument("current", type=Path, help="current scale-cliff JSON artifact")
    parser.add_argument("--json-file", type=Path, help="optional JSON report path")
    parser.add_argument("--markdown-file", type=Path, help="optional Markdown report path")
    parser.add_argument("--threshold", type=float, default=0.10, help="relative regression/improvement threshold")
    parser.add_argument(
        "--generated-at",
        default=datetime.now(timezone.utc).isoformat(),
        help="timestamp to store in report artifacts",
    )
    parser.add_argument(
        "--fail-on-regression",
        action="store_true",
        help="exit 1 when any ratio regresses by at least --threshold",
    )
    args = parser.parse_args(argv)

    report = build_report(
        load_artifact(args.previous),
        load_artifact(args.current),
        previous_path=args.previous,
        current_path=args.current,
        generated_at=args.generated_at,
        threshold=args.threshold,
    )

    if args.json_file:
        write_json(args.json_file, report)
    if args.markdown_file:
        write_markdown(args.markdown_file, report)
    if not args.json_file and not args.markdown_file:
        sys.stdout.write(markdown_report(report))

    if args.fail_on_regression and report["totals"]["regressions"] > 0:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
