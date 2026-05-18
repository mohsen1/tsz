#!/usr/bin/env python3
"""Report branch-local `visited.clone()` traversal sites.

This is a Performance Plan guardrail report, not a hard failure gate. It scans
compiler Rust sources for clones of variables whose names contain `visited`.
Those clones are often a signal that graph traversal is carrying branch-local
visited state instead of using node-keyed memoized DP, worklists, SCCs, or a
documented small bound.
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
    "crates/tsz-lsp/src",
    "crates/tsz-solver/src",
)
EXCLUDED_DIRS = {".git", "target", "node_modules", "tests", "benches", "examples"}
CLONE_RE = re.compile(
    r"\b(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s*\.\s*clone\s*\("
)


@dataclass(frozen=True)
class VisitedCloneHit:
    path: str
    line: int
    name: str
    text: str


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


def scan_file(path: Path) -> list[VisitedCloneHit]:
    hits: list[VisitedCloneHit] = []
    for line_no, raw_line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        code = strip_line_comment(raw_line)
        for match in CLONE_RE.finditer(code):
            name = match.group("name")
            if "visited" not in name.lower():
                continue
            hits.append(
                VisitedCloneHit(
                    path=relative(path),
                    line=line_no,
                    name=name,
                    text=raw_line.strip(),
                )
            )
    return hits


def scan(roots: Iterable[Path]) -> list[VisitedCloneHit]:
    hits: list[VisitedCloneHit] = []
    for path in iter_rust_files(roots):
        hits.extend(scan_file(path))
    return sorted(hits, key=lambda hit: (hit.path, hit.line, hit.name))


def summarize(hits: list[VisitedCloneHit]) -> dict[str, object]:
    by_path: dict[str, int] = {}
    by_name: dict[str, int] = {}
    for hit in hits:
        by_path[hit.path] = by_path.get(hit.path, 0) + 1
        by_name[hit.name] = by_name.get(hit.name, 0) + 1
    return {
        "schema_version": 1,
        "total": len(hits),
        "by_name": dict(sorted(by_name.items())),
        "by_path": dict(sorted(by_path.items())),
    }


def print_text(hits: list[VisitedCloneHit]) -> None:
    data = summarize(hits)
    print("visited clone report:")
    print(f"  total: {data['total']}")
    for name, count in data["by_name"].items():
        print(f"  {name}: {count}")
    if not hits:
        return
    print()
    for hit in hits:
        print(f"  {hit.path}:{hit.line}: {hit.name}: {hit.text}")


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
    hits = scan(roots)
    if args.json:
        print(
            json.dumps(
                {
                    "roots": [str(root) for root in roots],
                    "summary": summarize(hits),
                    "hits": [asdict(hit) for hit in hits],
                },
                indent=2,
                sort_keys=True,
            )
        )
    else:
        print_text(hits)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
