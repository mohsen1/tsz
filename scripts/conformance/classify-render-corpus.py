#!/usr/bin/env python3
"""Classify diagnostic render/fingerprint conformance failures.

This script is the first step in the diagnostic render conformance plan. It
turns the compact conformance detail snapshot plus optional verbose
`--print-fingerprints` runner output into stable buckets that can be tracked
between PRs.

Examples:
  # Coarse classification from the last snapshot.
  python3 scripts/conformance/classify-render-corpus.py

  # Rich fingerprint classification from a runner log.
  python3 scripts/conformance/classify-render-corpus.py \
    --fingerprint-log /tmp/tsz-fingerprint-deltas.txt

  # Save machine-readable outputs.
  python3 scripts/conformance/classify-render-corpus.py \
    --fingerprint-log /tmp/tsz-fingerprint-deltas.txt \
    --json-output /tmp/render-corpus.json \
    --csv-output /tmp/render-corpus.csv
"""

from __future__ import annotations

import argparse
import csv
import json
import re
import sys
from collections import Counter
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
DEFAULT_DETAIL = SCRIPT_DIR / "conformance-detail.json"

FINGERPRINT_RE = re.compile(
    r"^\s*-\s+(TS\d+)\s+(.+?):(\d+):(\d+)\s+(.*)$"
)


def normalize_code(code: str) -> str:
    code = str(code).strip()
    if not code:
        return code
    return code if code.startswith("TS") else f"TS{code}"


def normalize_path(path: str) -> str:
    path = path.strip()
    if path.startswith("./"):
        path = path[2:]
    return path


def basename(path: str) -> str:
    return path.rsplit("/", 1)[-1] if "/" in path else path


def area_of(path: str) -> str:
    markers = [
        "/cases/compiler/",
        "/cases/conformance/",
    ]
    for marker in markers:
        if marker in path:
            rest = path.split(marker, 1)[1]
            parts = rest.split("/")
            if len(parts) >= 2:
                return "/".join(parts[:-1])
            return "compiler"
    return ""


def load_detail(path: Path) -> dict:
    if not path.exists():
        raise SystemExit(
            f"error: {path} not found; run ./scripts/conformance/conformance.sh snapshot"
        )
    with path.open() as f:
        return json.load(f)


def resolve_log_path(path: str, detail_paths: set[str]) -> str:
    path = normalize_path(path)
    if path in detail_paths:
        return path
    prefixed = f"TypeScript/tests/cases/{path}"
    if prefixed in detail_paths:
        return prefixed
    return path


def parse_fingerprint(line: str) -> dict | None:
    match = FINGERPRINT_RE.match(line)
    if not match:
        return None
    code, file_name, line_no, column, message = match.groups()
    return {
        "code": code,
        "file": file_name,
        "line": int(line_no),
        "column": int(column),
        "message": message,
    }


def parse_fingerprint_log(path: Path, detail_paths: set[str]) -> dict[str, dict[str, list[dict]]]:
    """Parse runner `--print-fingerprints` output by failing test path."""
    result: dict[str, dict[str, list[dict]]] = {}
    current_path: str | None = None
    current_bucket: str | None = None

    with path.open(errors="replace") as f:
        for raw_line in f:
            line = raw_line.rstrip("\n")

            match = re.match(r"^FAIL\s+(.+?)(?:\s+\(ERROR: .+\))?$", line)
            if match:
                current_path = resolve_log_path(match.group(1), detail_paths)
                result.setdefault(current_path, {"missing": [], "extra": []})
                current_bucket = None
                continue

            if re.match(r"^(PASS|CRASH|TIMEOUT)\s+", line) or line.startswith("FINAL RESULTS:"):
                current_path = None
                current_bucket = None
                continue

            if current_path is None:
                continue

            stripped = line.strip()
            if stripped == "missing-fingerprints:":
                current_bucket = "missing"
                continue
            if stripped == "missing-fingerprints: []":
                current_bucket = None
                continue
            if stripped == "extra-fingerprints:":
                current_bucket = "extra"
                continue
            if stripped == "extra-fingerprints: []":
                current_bucket = None
                continue

            if current_bucket and stripped.startswith("- "):
                fingerprint = parse_fingerprint(line)
                if fingerprint:
                    result[current_path][current_bucket].append(fingerprint)

    return result


def tuple_counts(items: list[dict], key_fn) -> Counter:
    counts = Counter()
    for item in items:
        counts[key_fn(item)] += 1
    return counts


def same_multiset_by(missing: list[dict], extra: list[dict], key_fn) -> bool:
    return bool(missing or extra) and tuple_counts(missing, key_fn) == tuple_counts(extra, key_fn)


def classify_fingerprint_delta(missing: list[dict], extra: list[dict]) -> str:
    if not missing and not extra:
        return "fingerprint-unclassified"
    if missing and not extra:
        return "under-count"
    if extra and not missing:
        return "over-count"

    same_code_location = same_multiset_by(
        missing, extra, lambda fp: (fp["code"], fp["file"], fp["line"], fp["column"])
    )
    if same_code_location:
        return "message-only"

    same_location = same_multiset_by(
        missing, extra, lambda fp: (fp["file"], fp["line"], fp["column"])
    )
    if same_location:
        return "per-instance wrong code"

    same_code_message = same_multiset_by(
        missing, extra, lambda fp: (fp["code"], fp["message"])
    )
    if same_code_message:
        return "location-only"

    return "mixed"


def failure_category(failure: dict) -> str:
    expected = failure.get("e", [])
    actual = failure.get("a", [])
    missing = failure.get("m", [])
    extra = failure.get("x", [])

    if expected and actual and expected == actual:
        return "fingerprint-only"
    if not expected and actual:
        return "false-positive"
    if expected and not actual:
        return "all-missing"
    if missing or extra:
        return "wrong-code"
    return "unknown"


def code_filter_matches(record: dict, codes: set[str]) -> bool:
    if not codes:
        return True
    record_codes = set(record.get("codes", []))
    record_codes.update(record.get("missing_codes", []))
    record_codes.update(record.get("extra_codes", []))
    for fp in record.get("missing_fingerprints", []):
        record_codes.add(fp["code"])
    for fp in record.get("extra_fingerprints", []):
        record_codes.add(fp["code"])
    return bool(record_codes & codes)


def build_records(detail: dict, fingerprint_log: dict[str, dict[str, list[dict]]] | None) -> list[dict]:
    records = []
    failures = detail.get("failures", {})
    for path, failure in sorted(failures.items()):
        category = failure_category(failure)
        missing_fps: list[dict] = []
        extra_fps: list[dict] = []
        if fingerprint_log and path in fingerprint_log:
            missing_fps = fingerprint_log[path].get("missing", [])
            extra_fps = fingerprint_log[path].get("extra", [])

        if category == "fingerprint-only":
            render_class = classify_fingerprint_delta(missing_fps, extra_fps)
        else:
            render_class = category

        delta_codes = Counter()
        for fp in missing_fps:
            delta_codes[(fp["code"], "missing")] += 1
        for fp in extra_fps:
            delta_codes[(fp["code"], "extra")] += 1

        records.append(
            {
                "path": path,
                "name": basename(path),
                "area": area_of(path),
                "category": render_class,
                "base_category": category,
                "codes": failure.get("e", []),
                "actual_codes": failure.get("a", []),
                "missing_codes": failure.get("m", []),
                "extra_codes": failure.get("x", []),
                "missing_fingerprint_count": len(missing_fps),
                "extra_fingerprint_count": len(extra_fps),
                "delta_codes": [
                    {"code": code, "side": side, "count": count}
                    for (code, side), count in sorted(delta_codes.items())
                ],
                "missing_fingerprints": missing_fps,
                "extra_fingerprints": extra_fps,
            }
        )
    return records


def summarize(records: list[dict]) -> dict:
    category_counts = Counter(record["category"] for record in records)
    base_category_counts = Counter(record["base_category"] for record in records)
    code_deltas = Counter()
    class_code_counts: dict[str, Counter] = {}
    area_counts = Counter()

    for record in records:
        category = record["category"]
        class_code_counts.setdefault(category, Counter())
        if record["area"]:
            area_counts[record["area"]] += 1

        for code in record.get("codes", []):
            if record["base_category"] == "fingerprint-only":
                class_code_counts[category][code] += 1

        for fp in record.get("missing_fingerprints", []):
            code_deltas[(fp["code"], "missing")] += 1
        for fp in record.get("extra_fingerprints", []):
            code_deltas[(fp["code"], "extra")] += 1

    by_code = {}
    for (code, side), count in code_deltas.items():
        by_code.setdefault(code, {"code": code, "missing": 0, "extra": 0, "total": 0})
        by_code[code][side] = count
        by_code[code]["total"] += count

    total_fingerprint_only = base_category_counts.get("fingerprint-only", 0)
    classified_fingerprint_only = sum(
        count
        for category, count in category_counts.items()
        if category
        not in {
            "fingerprint-unclassified",
            "wrong-code",
            "all-missing",
            "false-positive",
            "unknown",
        }
    )

    return {
        "summary": {
            "total_failures": len(records),
            "fingerprint_only": total_fingerprint_only,
            "classified_fingerprint_only": classified_fingerprint_only,
            "unclassified_fingerprint_only": category_counts.get("fingerprint-unclassified", 0),
            "wrong_code": base_category_counts.get("wrong-code", 0),
            "all_missing": base_category_counts.get("all-missing", 0),
            "false_positive": base_category_counts.get("false-positive", 0),
        },
        "categories": [
            {"category": category, "tests": count}
            for category, count in category_counts.most_common()
        ],
        "fingerprint_delta_codes": sorted(
            by_code.values(), key=lambda item: (-item["total"], item["code"])
        ),
        "class_top_codes": {
            category: [
                {"code": code, "tests": count}
                for code, count in counts.most_common(10)
            ]
            for category, counts in sorted(class_code_counts.items())
        },
        "areas": [
            {"area": area, "tests": count} for area, count in area_counts.most_common(20)
        ],
    }


def print_summary(summary: dict, records: list[dict], top: int, paths_only: bool) -> None:
    if paths_only:
        for record in records:
            print(record["path"])
        return

    s = summary["summary"]
    print("Diagnostic render corpus")
    print("=" * 70)
    print(f"Failures:                  {s['total_failures']}")
    print(f"Fingerprint-only:          {s['fingerprint_only']}")
    print(f"Classified fingerprint-only: {s['classified_fingerprint_only']}")
    print(f"Unclassified fingerprint-only: {s['unclassified_fingerprint_only']}")
    print(f"Wrong-code:                {s['wrong_code']}")
    print(f"All-missing:               {s['all_missing']}")
    print(f"False-positive:            {s['false_positive']}")
    print()

    print("Categories:")
    for item in summary["categories"]:
        print(f"  {item['category']:<28} {item['tests']:>5}")
    print()

    if summary["fingerprint_delta_codes"]:
        print("Top fingerprint delta codes:")
        for item in summary["fingerprint_delta_codes"][:10]:
            print(
                f"  {item['code']:>8} total={item['total']:>4} "
                f"missing={item['missing']:>4} extra={item['extra']:>4}"
            )
        print()

    interesting = [
        record
        for record in records
        if record["base_category"] == "fingerprint-only"
        and (
            record["missing_fingerprint_count"]
            or record["extra_fingerprint_count"]
            or record["category"] == "fingerprint-unclassified"
        )
    ]
    interesting.sort(
        key=lambda record: (
            -(record["missing_fingerprint_count"] + record["extra_fingerprint_count"]),
            record["category"],
            record["name"].lower(),
        )
    )

    print("Representative fingerprint-only tests:")
    for record in interesting[:top]:
        codes = ",".join(record["codes"])
        total = record["missing_fingerprint_count"] + record["extra_fingerprint_count"]
        print(
            f"  {record['category']:<24} deltas={total:>3} "
            f"missing={record['missing_fingerprint_count']:>2} "
            f"extra={record['extra_fingerprint_count']:>2} "
            f"codes=[{codes}] {record['name']}"
        )
    if len(interesting) > top:
        print(f"  ... and {len(interesting) - top} more")


def write_json(path: Path, summary: dict, records: list[dict]) -> None:
    payload = {**summary, "tests": records}
    with path.open("w") as f:
        json.dump(payload, f, indent=2)
        f.write("\n")


def write_csv(path: Path, records: list[dict]) -> None:
    fieldnames = [
        "path",
        "name",
        "area",
        "category",
        "base_category",
        "codes",
        "actual_codes",
        "missing_codes",
        "extra_codes",
        "missing_fingerprint_count",
        "extra_fingerprint_count",
        "delta_codes",
    ]
    with path.open("w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        for record in records:
            row = {key: record.get(key, "") for key in fieldnames}
            for key in ["codes", "actual_codes", "missing_codes", "extra_codes"]:
                row[key] = " ".join(row[key])
            row["delta_codes"] = " ".join(
                f"{item['code']}:{item['side']}:{item['count']}"
                for item in record.get("delta_codes", [])
            )
            writer.writerow(row)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Classify diagnostic render/fingerprint conformance failures"
    )
    parser.add_argument(
        "--detail",
        type=Path,
        default=DEFAULT_DETAIL,
        help="Path to conformance-detail.json",
    )
    parser.add_argument(
        "--fingerprint-log",
        type=Path,
        help="Verbose conformance runner output produced with --print-fingerprints",
    )
    parser.add_argument("--json-output", type=Path, help="Write JSON corpus")
    parser.add_argument("--csv-output", type=Path, help="Write CSV corpus")
    parser.add_argument(
        "--code",
        action="append",
        default=[],
        help="Restrict printed/exported records to a diagnostic code, e.g. TS2322",
    )
    parser.add_argument("--top", type=int, default=25, help="Rows to show in text output")
    parser.add_argument("--paths-only", action="store_true", help="Print only matching paths")
    args = parser.parse_args()

    detail = load_detail(args.detail)
    detail_paths = set(detail.get("failures", {}).keys())

    fingerprint_log = None
    if args.fingerprint_log:
        if not args.fingerprint_log.exists():
            raise SystemExit(f"error: fingerprint log not found: {args.fingerprint_log}")
        fingerprint_log = parse_fingerprint_log(args.fingerprint_log, detail_paths)

    records = build_records(detail, fingerprint_log)
    codes = {normalize_code(code) for code in args.code}
    records = [record for record in records if code_filter_matches(record, codes)]
    summary = summarize(records)

    print_summary(summary, records, args.top, args.paths_only)

    if args.json_output:
        write_json(args.json_output, summary, records)
    if args.csv_output:
        write_csv(args.csv_output, records)

    if not args.fingerprint_log and summary["summary"]["fingerprint_only"]:
        print(
            "\nnote: pass --fingerprint-log with runner output from --print-fingerprints "
            "to split fingerprint-only tests into message/count/location buckets.",
            file=sys.stderr,
        )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
