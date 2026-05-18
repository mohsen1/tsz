#!/usr/bin/env python3
"""Report cache-like fields that need stats or size-accounting review.

This is a Performance Plan guardrail report, not a hard failure gate. It scans
compiler Rust sources for cache-like map fields/type aliases and annotates each
candidate with simple evidence that hit/miss/entry statistics or size/memory
accounting exist nearby.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Iterable


ROOT = Path(__file__).resolve().parents[2]

DEFAULT_ROOTS = (
    "crates/tsz-binder/src",
    "crates/tsz-checker/src",
    "crates/tsz-core/src",
    "crates/tsz-lsp/src",
    "crates/tsz-solver/src",
)
EXCLUDED_DIRS = {".git", "target", "node_modules", "tests", "benches", "examples"}
MAP_TYPES = ("HashMap", "FxHashMap", "DashMap", "IndexMap", "BTreeMap")

FIELD_RE = re.compile(
    r"^\s*(?:(?:pub|pub\(crate\)|pub\(super\)|pub\(in [^)]+\))\s+)?"
    r"(?P<name>[A-Za-z_][A-Za-z0-9_]*(?:cache|Cache|memo|Memo)[A-Za-z0-9_]*)"
    r"\s*:\s*(?P<type>.+)$"
)
TYPE_ALIAS_RE = re.compile(
    r"^\s*(?:(?:pub|pub\(crate\)|pub\(super\)|pub\(in [^)]+\))\s+)?"
    r"type\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*(?:Cache|cache|Memo|memo)[A-Za-z0-9_]*)"
    r"\s*=\s*(?P<type>.+)$"
)
STRUCT_RE = re.compile(
    r"^\s*(?:(?:pub|pub\(crate\)|pub\(super\)|pub\(in [^)]+\))\s+)?"
    r"struct\s+([A-Za-z_][A-Za-z0-9_]*)\b"
)
STATS_RE = re.compile(r"(?:hits?|miss(?:es)?|entries|entry_count|Stats|statistics|total_entries)")
SIZE_RE = re.compile(r"(?:estimated_size_bytes|size_bytes|memory|Memory|total_entries|entries)")


@dataclass(frozen=True)
class CacheCandidate:
    path: str
    line: int
    owner: str
    name: str
    type: str
    stats_signal: bool
    size_signal: bool

    @property
    def needs_review(self) -> bool:
        return not (self.stats_signal and self.size_signal)


def relative(path: Path) -> str:
    try:
        return path.relative_to(ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def is_ignored(path: Path, root: Path) -> bool:
    rel_parts = set(path.relative_to(root).parts)
    if EXCLUDED_DIRS.intersection(rel_parts):
        return True
    name = path.name
    return name.endswith("_test.rs") or name.endswith("_tests.rs") or name == "test_utils.rs"


def iter_rust_files(roots: Iterable[Path]) -> Iterable[Path]:
    for root in roots:
        if not root.exists():
            continue
        for path in sorted(root.rglob("*.rs")):
            if not is_ignored(path, root):
                yield path


def strip_line_comment(line: str) -> str:
    return line.split("//", 1)[0]


def trim_type(raw: str) -> str:
    return raw.strip().rstrip(",").strip()


def is_cache_type(type_text: str) -> bool:
    return any(map_type in type_text for map_type in MAP_TYPES)


def stat_stem(name: str) -> str:
    stem = re.sub(r"(?:_?cache|_?memo)$", "", name, flags=re.IGNORECASE)
    return stem or name


def has_stats_signal(file_text: str, name: str) -> bool:
    stem = re.escape(stat_stem(name))
    name_re = re.escape(name)
    patterns = (
        rf"{stem}.*(?:hits?|miss(?:es)?|entries)",
        rf"(?:hits?|miss(?:es)?|entries).*{stem}",
        rf"{name_re}\.len\s*\(",
    )
    return any(re.search(pattern, file_text, re.IGNORECASE) for pattern in patterns)


def has_size_signal(file_text: str, name: str) -> bool:
    stem = re.escape(stat_stem(name))
    name_re = re.escape(name)
    patterns = (
        rf"{stem}.*(?:estimated_size_bytes|size_bytes|memory|total_entries|entries)",
        rf"(?:estimated_size_bytes|size_bytes|memory|total_entries|entries).*{stem}",
        rf"{name_re}\.len\s*\(",
    )
    return any(re.search(pattern, file_text, re.IGNORECASE) for pattern in patterns)


def scan_file(path: Path) -> list[CacheCandidate]:
    raw_text = path.read_text(encoding="utf-8")
    file_text = "\n".join(strip_line_comment(line) for line in raw_text.splitlines())
    rel = relative(path)
    owner = "<module>"
    struct_depth = 0
    candidates: list[CacheCandidate] = []
    for line_no, raw_line in enumerate(raw_text.splitlines(), start=1):
        line = strip_line_comment(raw_line)
        struct_match = STRUCT_RE.search(line)
        if struct_match:
            owner = struct_match.group(1)
            struct_depth = max(1, line.count("{") - line.count("}"))

        alias_match = TYPE_ALIAS_RE.search(line)
        match = alias_match
        if match is None and struct_depth > 0:
            match = FIELD_RE.search(line)
        if not match:
            if struct_depth > 0 and not struct_match:
                struct_depth += line.count("{") - line.count("}")
                if struct_depth <= 0:
                    owner = "<module>"
            continue
        name = match.group("name")
        type_text = trim_type(match.group("type"))
        if not is_cache_type(type_text):
            if struct_depth > 0 and not struct_match:
                struct_depth += line.count("{") - line.count("}")
                if struct_depth <= 0:
                    owner = "<module>"
            continue
        candidates.append(
            CacheCandidate(
                path=rel,
                line=line_no,
                owner=owner,
                name=name,
                type=type_text,
                stats_signal=has_stats_signal(file_text, name),
                size_signal=has_size_signal(file_text, name),
            )
        )
        if struct_depth > 0 and not struct_match:
            struct_depth += line.count("{") - line.count("}")
            if struct_depth <= 0:
                owner = "<module>"
    return candidates


def scan(roots: Iterable[Path]) -> list[CacheCandidate]:
    candidates: list[CacheCandidate] = []
    for path in iter_rust_files(roots):
        candidates.extend(scan_file(path))
    return sorted(candidates, key=lambda c: (c.path, c.line, c.name))


def summarize(candidates: list[CacheCandidate]) -> dict[str, object]:
    by_path: dict[str, int] = {}
    by_owner: dict[str, int] = {}
    for candidate in candidates:
        if candidate.needs_review:
            by_path[candidate.path] = by_path.get(candidate.path, 0) + 1
            by_owner[candidate.owner] = by_owner.get(candidate.owner, 0) + 1
    return {
        "schema_version": 1,
        "total_candidates": len(candidates),
        "needs_review": sum(1 for candidate in candidates if candidate.needs_review),
        "with_stats_signal": sum(1 for candidate in candidates if candidate.stats_signal),
        "with_size_signal": sum(1 for candidate in candidates if candidate.size_signal),
        "needs_review_by_path": dict(sorted(by_path.items())),
        "needs_review_by_owner": dict(sorted(by_owner.items())),
    }


def print_text(candidates: list[CacheCandidate]) -> None:
    summary = summarize(candidates)
    print("cache visibility report:")
    print(f"  total_candidates: {summary['total_candidates']}")
    print(f"  needs_review: {summary['needs_review']}")
    print(f"  with_stats_signal: {summary['with_stats_signal']}")
    print(f"  with_size_signal: {summary['with_size_signal']}")
    if not candidates:
        return
    print()
    print("candidates:")
    for candidate in candidates:
        marker = "review" if candidate.needs_review else "covered"
        print(
            f"  {marker} {candidate.path}:{candidate.line} "
            f"{candidate.owner}.{candidate.name} "
            f"stats={str(candidate.stats_signal).lower()} "
            f"size={str(candidate.size_signal).lower()} "
            f"type={candidate.type}"
        )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        action="append",
        type=Path,
        default=None,
        help="source root to scan; may be repeated",
    )
    parser.add_argument("--json", action="store_true", help="print machine-readable JSON")
    args = parser.parse_args(argv)

    roots = args.root if args.root is not None else [ROOT / root for root in DEFAULT_ROOTS]
    candidates = scan(roots)
    if args.json:
        print(
            json.dumps(
                {
                    "roots": [str(root) for root in roots],
                    "summary": summarize(candidates),
                    "candidates": [asdict(candidate) for candidate in candidates],
                },
                indent=2,
                sort_keys=True,
            )
        )
    else:
        print_text(candidates)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
