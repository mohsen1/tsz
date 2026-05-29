#!/usr/bin/env python3
"""Audit emitter output-surgery rewrites.

This script is intentionally conservative: harmless string-data cleanup is
allowed automatically, while semantic rewrites over already-emitted JS/DTS are
treated as ratcheted debt. Current debt is listed in
`output-surgery-allowlist.txt` by file, category, max count, and reason.
"""

from __future__ import annotations

import argparse
import dataclasses
import json
import pathlib
import re
import sys
from collections import Counter, defaultdict


ROOT = pathlib.Path(__file__).resolve().parents[2]
SOURCE_ROOT = ROOT / "crates" / "tsz-emitter" / "src"
ALLOWLIST_PATH = ROOT / "scripts" / "emit" / "output-surgery-allowlist.txt"

REPLACE_CALL_RE = re.compile(r"(?:\.\s*)?(replace|replacen|replace_range)\s*\(")
UNALLOWLISTED_FAILURE_RE = re.compile(
    r": (?P<count>\d+) unallowlisted output-surgery call\(s\)"
)
OVER_ALLOWLIST_FAILURE_RE = re.compile(
    r": (?P<count>\d+) output-surgery call\(s\), allowlist max is (?P<max_count>\d+)"
)


@dataclasses.dataclass(frozen=True)
class Finding:
    path: str
    line_no: int
    call: str
    text: str


@dataclasses.dataclass(frozen=True)
class AllowEntry:
    category: str
    max_count: int
    reason: str


@dataclasses.dataclass(frozen=True)
class FailureSummary:
    # Backward-compatible top-level counters:
    # - unallowlisted counts calls, matching the guardrail debt metric.
    # - over_allowlist and stale_allowlist count affected allowlist rows.
    unallowlisted: int = 0
    over_allowlist: int = 0
    stale_allowlist: int = 0
    unallowlisted_files: int = 0
    over_allowlist_files: int = 0
    over_allowlist_excess_calls: int = 0
    stale_allowlist_files: int = 0


@dataclasses.dataclass(frozen=True)
class BudgetSummary:
    allowlisted_calls: int = 0
    allowlist_cap: int = 0
    remaining_allowlist_capacity: int = 0
    allowlisted_files: int = 0
    budget_status: str = "no_allowlist"


def iter_rust_files(base: pathlib.Path = SOURCE_ROOT):
    yield from sorted(base.rglob("*.rs"))


def is_auto_allowed_data_cleanup(path: str, line: str) -> bool:
    stripped = line.strip()

    # State mutation APIs with the same name are not output string surgery.
    if "std::mem::replace" in stripped:
        return True
    if re.search(r"\b[a-zA-Z_][a-zA-Z0-9_]*\.replace\(", stripped) and not re.search(
        r"\b(output|emitted|rewritten|type_text|constructor_type|line|assignment|remainder)\.replace",
        stripped,
    ):
        return True
    if stripped.startswith(".replace(") and not re.search(r"\.replace\([\"'&]", stripped):
        return True

    # Runtime helper source text intentionally contains JavaScript `.replace(...)`.
    if path.endswith("transforms/helpers.rs") and "return path.replace(" in stripped:
        return True

    # Escaping and literal normalization are data construction, not emitted
    # program-structure surgery.
    data_cleanup_needles = [
        ".replace('\\\\',",
        '.replace("\\\\",',
        ".replace('\"',",
        ".replace(\"\\\"\",",
        ".replace('\\'',",
        ".replace('_', \"\")",
        ".replace(\"\\r\\n\",",
        ".replace('\\r',",
        ".replace('\\n',",
        ".replace('*',",
        '.replace("*/"',
    ]
    if any(needle in stripped for needle in data_cleanup_needles):
        return True

    # Path normalization helpers often receive owned path text as data.
    if ".replace('\\\\', \"/\")" in stripped or "../node_modules/" in stripped:
        return True

    return False


def scan(base: pathlib.Path = SOURCE_ROOT) -> list[Finding]:
    findings: list[Finding] = []
    for path in iter_rust_files(base):
        rel = path.relative_to(ROOT).as_posix()
        for line_no, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
            for match in REPLACE_CALL_RE.finditer(line):
                if is_auto_allowed_data_cleanup(rel, line):
                    continue
                findings.append(
                    Finding(
                        path=rel,
                        line_no=line_no,
                        call=match.group(1),
                        text=line.strip(),
                    )
                )
    return findings


def load_allowlist(path: pathlib.Path = ALLOWLIST_PATH) -> dict[str, AllowEntry]:
    entries: dict[str, AllowEntry] = {}
    if not path.exists():
        return entries
    for line_no, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split("|", 3)
        if len(parts) != 4:
            raise ValueError(f"{path}:{line_no}: expected path|category|max_count|reason")
        rel_path, category, max_count, reason = [part.strip() for part in parts]
        if not rel_path or not category or not reason:
            raise ValueError(f"{path}:{line_no}: path, category, and reason are required")
        entries[rel_path] = AllowEntry(
            category=category,
            max_count=int(max_count),
            reason=reason,
        )
    return entries


def grouped_counts(findings: list[Finding]) -> Counter[str]:
    return Counter(finding.path for finding in findings)


def audit(findings: list[Finding], allowlist: dict[str, AllowEntry]) -> list[str]:
    failures: list[str] = []
    counts = grouped_counts(findings)
    for path, count in sorted(counts.items()):
        entry = allowlist.get(path)
        if entry is None:
            failures.append(f"{path}: {count} unallowlisted output-surgery call(s)")
        elif count > entry.max_count:
            failures.append(
                f"{path}: {count} output-surgery call(s), allowlist max is {entry.max_count}"
            )
    for path in sorted(set(allowlist) - set(counts)):
        failures.append(f"{path}: allowlist entry is stale; no matching calls remain")
    return failures


def summarize_failures(failures: list[str]) -> FailureSummary:
    summary = FailureSummary()
    for failure in failures:
        if unallowlisted_match := UNALLOWLISTED_FAILURE_RE.search(failure):
            summary = dataclasses.replace(
                summary,
                unallowlisted=summary.unallowlisted
                + int(unallowlisted_match.group("count")),
                unallowlisted_files=summary.unallowlisted_files + 1,
            )
        elif "allowlist entry is stale" in failure:
            summary = dataclasses.replace(
                summary,
                stale_allowlist=summary.stale_allowlist + 1,
                stale_allowlist_files=summary.stale_allowlist_files + 1,
            )
        elif over_allowlist_match := OVER_ALLOWLIST_FAILURE_RE.search(failure):
            count = int(over_allowlist_match.group("count"))
            max_count = int(over_allowlist_match.group("max_count"))
            summary = dataclasses.replace(
                summary,
                over_allowlist=summary.over_allowlist + 1,
                over_allowlist_files=summary.over_allowlist_files + 1,
                over_allowlist_excess_calls=summary.over_allowlist_excess_calls
                + max(0, count - max_count),
            )
    return summary


def file_status(path: str, count: int, allowlist: dict[str, AllowEntry]) -> str:
    entry = allowlist.get(path)
    if entry is None:
        return "unallowlisted"
    if count == 0:
        return "stale_allowlist"
    if count > entry.max_count:
        return "over_allowlist"
    return "allowlisted"


def build_file_summaries(
    counts: Counter[str],
    allowlist: dict[str, AllowEntry],
) -> list[dict[str, object]]:
    summaries: list[dict[str, object]] = []
    for path in sorted(set(counts) | set(allowlist)):
        entry = allowlist.get(path)
        count = counts.get(path, 0)
        summaries.append(
            {
                "path": path,
                "count": count,
                "category": entry.category if entry else "UNALLOWLISTED",
                "max_count": entry.max_count if entry else None,
                "reason": entry.reason if entry else None,
                "status": file_status(path, count, allowlist),
            }
        )
    return summaries


def build_category_summaries(file_summaries: list[dict[str, object]]) -> list[dict[str, object]]:
    categories: dict[str, dict[str, object]] = {}
    for summary in file_summaries:
        category = str(summary["category"])
        entry = categories.setdefault(
            category,
            {
                "category": category,
                "count": 0,
                "max_count": 0,
                "files": 0,
                "statuses": Counter(),
            },
        )
        entry["count"] = int(entry["count"]) + int(summary["count"])
        max_count = summary["max_count"]
        if max_count is None:
            entry["max_count"] = None
        elif entry["max_count"] is not None:
            entry["max_count"] = int(entry["max_count"]) + int(max_count)
        entry["files"] = int(entry["files"]) + 1
        entry["statuses"][str(summary["status"])] += 1

    result: list[dict[str, object]] = []
    for entry in sorted(categories.values(), key=lambda item: str(item["category"])):
        result.append(
            {
                "category": entry["category"],
                "count": entry["count"],
                "max_count": entry["max_count"],
                "files": entry["files"],
                "statuses": dict(sorted(entry["statuses"].items())),
            }
        )
    return result


def summarize_budget(file_summaries: list[dict[str, object]]) -> BudgetSummary:
    allowlisted_calls = 0
    allowlist_cap = 0
    allowlisted_files = 0
    for summary in file_summaries:
        max_count = summary["max_count"]
        if max_count is None:
            continue
        allowlisted_calls += int(summary["count"])
        allowlist_cap += int(max_count)
        allowlisted_files += 1
    return BudgetSummary(
        allowlisted_calls=allowlisted_calls,
        allowlist_cap=allowlist_cap,
        remaining_allowlist_capacity=max(0, allowlist_cap - allowlisted_calls),
        allowlisted_files=allowlisted_files,
        budget_status=classify_budget_status(allowlisted_calls, allowlist_cap),
    )


def classify_budget_status(allowlisted_calls: int, allowlist_cap: int) -> str:
    if allowlist_cap == 0:
        return "no_allowlist"
    if allowlisted_calls > allowlist_cap:
        return "over_cap"
    if allowlisted_calls == allowlist_cap:
        return "exhausted"
    return "available"


def format_budget_metrics(budget: BudgetSummary) -> str:
    return (
        f"allowlisted_calls={budget.allowlisted_calls}, "
        f"allowlist_cap={budget.allowlist_cap}, "
        f"remaining_allowlist_capacity={budget.remaining_allowlist_capacity}, "
        f"allowlist_budget_status={budget.budget_status}"
    )


def build_json_report(
    findings: list[Finding],
    allowlist: dict[str, AllowEntry],
    failures: list[str],
) -> dict[str, object]:
    counts = grouped_counts(findings)
    summary = summarize_failures(failures)
    file_summaries = build_file_summaries(counts, allowlist)
    return {
        "ok": not failures,
        "total_findings": len(findings),
        "files_with_findings": len(counts),
        "failure_summary": dataclasses.asdict(summary),
        "budget_summary": dataclasses.asdict(summarize_budget(file_summaries)),
        "failures": failures,
        "categories": build_category_summaries(file_summaries),
        "files": file_summaries,
        "findings": [
            {
                "path": finding.path,
                "line_no": finding.line_no,
                "call": finding.call,
                "category": allowlist[finding.path].category
                if finding.path in allowlist
                else "UNALLOWLISTED",
                "text": finding.text,
            }
            for finding in findings
        ],
    }


def write_json_report(path: pathlib.Path, report: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temp_path = path.with_name(f".{path.name}.tmp")
    temp_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    temp_path.replace(path)


def print_report(findings: list[Finding], allowlist: dict[str, AllowEntry]) -> None:
    by_path: dict[str, list[Finding]] = defaultdict(list)
    for finding in findings:
        by_path[finding.path].append(finding)

    for path in sorted(by_path):
        entry = allowlist.get(path)
        category = entry.category if entry else "UNALLOWLISTED"
        print(f"{path} [{category}] ({len(by_path[path])})")
        for finding in by_path[path]:
            print(f"  {finding.line_no}: {finding.text}")


def format_pass_summary(
    findings: list[Finding],
    failures: list[str],
    allowlist: dict[str, AllowEntry],
) -> str:
    summary = summarize_failures(failures)
    budget = summarize_budget(build_file_summaries(grouped_counts(findings), allowlist))
    return (
        "Output-surgery audit passed: "
        f"total_findings={len(findings)}, "
        f"files_with_findings={len(grouped_counts(findings))}, "
        f"{format_budget_metrics(budget)}, "
        f"unallowlisted_calls={summary.unallowlisted}, "
        f"over_allowlist_files={summary.over_allowlist_files}, "
        f"over_allowlist_excess_calls={summary.over_allowlist_excess_calls}, "
        f"stale_allowlist_files={summary.stale_allowlist_files}."
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--list", action="store_true", help="print all tracked findings")
    parser.add_argument(
        "--json-report",
        type=pathlib.Path,
        help="write a machine-readable report before exiting",
    )
    args = parser.parse_args(argv)

    findings = scan()
    allowlist = load_allowlist()
    failures = audit(findings, allowlist)

    if args.json_report is not None:
        write_json_report(args.json_report, build_json_report(findings, allowlist, failures))

    if args.list or failures:
        print_report(findings, allowlist)

    if failures:
        summary = summarize_failures(failures)
        budget = summarize_budget(build_file_summaries(grouped_counts(findings), allowlist))
        print(
            "\nOutput-surgery audit summary: "
            f"{format_budget_metrics(budget)}, "
            f"unallowlisted_calls={summary.unallowlisted}, "
            f"unallowlisted_files={summary.unallowlisted_files}, "
            f"over_allowlist_files={summary.over_allowlist_files}, "
            f"over_allowlist_excess_calls={summary.over_allowlist_excess_calls}, "
            f"stale_allowlist_files={summary.stale_allowlist_files}",
            file=sys.stderr,
        )
        print("\nOutput-surgery audit failed:", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1

    print(format_pass_summary(findings, failures, allowlist))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
