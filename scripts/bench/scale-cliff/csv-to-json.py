#!/usr/bin/env python3
"""Convert scale-cliff CSV output into a compact JSON artifact."""

from __future__ import annotations

import argparse
import csv
import json
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

INT_FIELDS = {
    "files",
    "exit_code",
    "memory_kb",
    "delegate_calls",
    "delegate_misses",
    "delegate_lib_hits",
    "delegate_xfile_hits",
    "checker_state_new",
    "checker_with_parent_cache",
    "overlay_copy_calls",
    "overlay_entries_copied",
    "compute_type_of_symbol_calls",
    "resolver_lookup_calls",
    "resolver_pj_reads",
}

FLOAT_FIELDS = {
    "total_s",
    "check_s",
    "parse_bind_s",
    "io_read_s",
    "ratio_checkers_per_file",
    "ratio_overlay_per_file",
    "ratio_delegations_per_file",
    "ratio_compute_per_file",
}

RATIO_FIELDS = (
    "ratio_checkers_per_file",
    "ratio_overlay_per_file",
    "ratio_delegations_per_file",
    "ratio_compute_per_file",
)


def parse_cell(key: str, value: str) -> Any:
    if key in INT_FIELDS:
        return int(value or 0)
    if key in FLOAT_FIELDS:
        return float(value or 0)
    return value


def load_rows(csv_path: Path) -> list[dict[str, Any]]:
    with csv_path.open(newline="", encoding="utf-8") as handle:
        return [
            {key: parse_cell(key, value) for key, value in row.items()}
            for row in csv.DictReader(handle)
        ]


def max_ratio(rows: list[dict[str, Any]], field: str) -> dict[str, Any] | None:
    if not rows:
        return None
    row = max(rows, key=lambda candidate: candidate[field])
    return {"fixture": row["fixture"], "value": row[field]}


def build_payload(
    rows: list[dict[str, Any]],
    *,
    csv_path: Path,
    generated_at: str,
    tsz_bin: str | None,
) -> dict[str, Any]:
    return {
        "schema_version": 1,
        "generated_at": generated_at,
        "csv_file": str(csv_path),
        "tsz_bin": tsz_bin,
        "fixtures": len(rows),
        "rows": rows,
        "max_ratios": {field: max_ratio(rows, field) for field in RATIO_FIELDS},
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("csv", type=Path, help="scale-cliff CSV path")
    parser.add_argument("--json-file", type=Path, required=True, help="JSON output path")
    parser.add_argument("--tsz-bin", default=None, help="tsz binary used for the run")
    parser.add_argument(
        "--generated-at",
        default=datetime.now(timezone.utc).isoformat(),
        help="timestamp to store in the JSON artifact",
    )
    args = parser.parse_args(argv)

    rows = load_rows(args.csv)
    payload = build_payload(
        rows,
        csv_path=args.csv,
        generated_at=args.generated_at,
        tsz_bin=args.tsz_bin,
    )
    args.json_file.parent.mkdir(parents=True, exist_ok=True)
    args.json_file.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
