#!/usr/bin/env python3
"""Report checker migration call-site counts for performance PR evidence."""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_ROOT = ROOT / "crates" / "tsz-checker" / "src"
EXCLUDED_DIRS = {".git", "target", "node_modules", "tests", "benches"}

PATTERNS = {
    "with_parent_cache": re.compile(r"\bwith_parent_cache\s*\("),
    "with_parent_cache_attributed": re.compile(r"\bwith_parent_cache_attributed\s*\("),
    "copy_symbol_file_targets_to": re.compile(r"\bcopy_symbol_file_targets_to\s*\("),
    "copy_symbol_file_targets_to_attributed": re.compile(
        r"\bcopy_symbol_file_targets_to_attributed\s*\("
    ),
}


@dataclass(frozen=True)
class Hit:
    metric: str
    rel: str
    line: int


def relative_to_root(path: Path, root: Path) -> str:
    try:
        return path.relative_to(ROOT).as_posix()
    except ValueError:
        return path.relative_to(root).as_posix()


def iter_rust_files(search_roots: Iterable[Path]) -> Iterable[tuple[Path, Path]]:
    for root in search_roots:
        if not root.exists():
            continue
        for path in root.rglob("*.rs"):
            parts = set(path.relative_to(root).parts)
            if EXCLUDED_DIRS.intersection(parts):
                continue
            yield root, path


def line_is_ignored(line: str) -> bool:
    stripped = line.lstrip()
    return stripped.startswith("//") or stripped.startswith("*") or re.match(
        r"(?:pub(?:\([^)]*\))?\s+)?fn\s+", stripped
    )


def scan(search_roots: Iterable[Path]) -> list[Hit]:
    hits: list[Hit] = []
    for root, path in iter_rust_files(search_roots):
        rel = relative_to_root(path, root)
        for line_no, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
            if line_is_ignored(line):
                continue
            for metric, pattern in PATTERNS.items():
                if pattern.search(line):
                    hits.append(Hit(metric=metric, rel=rel, line=line_no))
    return sorted(hits, key=lambda hit: (hit.metric, hit.rel, hit.line))


def summarize(hits: list[Hit]) -> dict[str, object]:
    counts = {metric: 0 for metric in PATTERNS}
    files: dict[str, dict[str, int]] = {}
    for hit in hits:
        counts[hit.metric] += 1
        files.setdefault(hit.rel, {metric: 0 for metric in PATTERNS})[hit.metric] += 1
    return {
        "schema_version": 1,
        "counts": counts,
        "files": dict(sorted(files.items())),
    }


def print_text(summary: dict[str, object]) -> None:
    counts = summary["counts"]
    files = summary["files"]
    print("migration call-site counts:")
    for metric in PATTERNS:
        print(f"  {metric}: {counts[metric]}")
    print()
    print("files:")
    for rel, file_counts in files.items():
        active = ", ".join(
            f"{metric}={count}" for metric, count in file_counts.items() if count
        )
        print(f"  {rel}: {active}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        action="append",
        type=Path,
        default=None,
        help="source root to scan; defaults to crates/tsz-checker/src",
    )
    parser.add_argument("--json", action="store_true", help="print machine-readable JSON")
    args = parser.parse_args(argv)

    search_roots = args.root if args.root is not None else [DEFAULT_ROOT]
    summary = summarize(scan(search_roots))
    if args.json:
        print(json.dumps(summary, indent=2))
    else:
        print_text(summary)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
