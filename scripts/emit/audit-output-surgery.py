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
import pathlib
import re
import sys
from collections import Counter, defaultdict


ROOT = pathlib.Path(__file__).resolve().parents[2]
SOURCE_ROOT = ROOT / "crates" / "tsz-emitter" / "src"
ALLOWLIST_PATH = ROOT / "scripts" / "emit" / "output-surgery-allowlist.txt"

REPLACE_CALL_RE = re.compile(r"(?:\.\s*)?(replace|replacen|replace_range)\s*\(")


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


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--list", action="store_true", help="print all tracked findings")
    args = parser.parse_args(argv)

    findings = scan()
    allowlist = load_allowlist()
    failures = audit(findings, allowlist)

    if args.list or failures:
        print_report(findings, allowlist)

    if failures:
        print("\nOutput-surgery audit failed:", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1

    print(
        f"Output-surgery audit passed: {len(findings)} ratcheted call(s) across "
        f"{len(grouped_counts(findings))} file(s)."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
