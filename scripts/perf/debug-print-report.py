#!/usr/bin/env python3
"""Report stdout/stderr debug macros in compiler internals.

This is a Performance Plan guardrail report, not a hard failure gate. It scans
compiler-internal Rust sources for `println!`, `eprintln!`, and `dbg!` so PRs
can cite the remaining surface and avoid adding ad-hoc debug output in hot
paths. Intentional CLI/user-facing output is out of scope.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import asdict, dataclass
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent.parent

DEFAULT_SCAN_DIRS = (
    "crates/tsz-binder/src",
    "crates/tsz-checker/src",
    "crates/tsz-common/src",
    "crates/tsz-core/src",
    "crates/tsz-emitter/src",
    "crates/tsz-lowering/src",
    "crates/tsz-parser/src",
    "crates/tsz-scanner/src",
    "crates/tsz-solver/src",
)

MACRO_RE = re.compile(r"\b(println|eprintln|dbg)!\s*(?:\(|\{|\[)")
COMMENT_PREFIXES = ("//", "///", "//!")


@dataclass(frozen=True)
class DebugPrintHit:
    path: str
    line: int
    macro: str
    text: str


def scrub_comments_and_strings(line: str, in_block_comment: bool) -> tuple[str, bool]:
    """Return code with comments/strings removed, preserving macro tokens."""
    escaped = False
    in_string = False
    in_char = False
    chars: list[str] = []
    i = 0
    while i < len(line):
        ch = line[i]
        nxt = line[i + 1] if i + 1 < len(line) else ""
        if in_block_comment:
            if ch == "*" and nxt == "/":
                in_block_comment = False
                i += 2
            else:
                i += 1
            continue
        if escaped:
            chars.append(" ")
            escaped = False
            i += 1
            continue
        if ch == "\\" and (in_string or in_char):
            chars.append(" ")
            escaped = True
            i += 1
            continue
        if ch == '"' and not in_char:
            in_string = not in_string
            chars.append(" ")
            i += 1
            continue
        if ch == "'" and not in_string:
            in_char = not in_char
            chars.append(" ")
            i += 1
            continue
        if ch == "/" and nxt == "*" and not in_string and not in_char:
            in_block_comment = True
            i += 2
            continue
        if ch == "/" and nxt == "/" and not in_string and not in_char:
            break
        chars.append(" " if in_string or in_char else ch)
        i += 1
    return "".join(chars), in_block_comment


def is_test_path(path: Path) -> bool:
    parts = set(path.parts)
    if "tests" in parts or "benches" in parts or "examples" in parts:
        return True
    name = path.name
    return name.endswith("_test.rs") or name.endswith("_tests.rs")


def iter_rust_files(root: Path, scan_dirs: tuple[str, ...]) -> list[Path]:
    files: list[Path] = []
    for scan_dir in scan_dirs:
        base = root / scan_dir
        if not base.exists():
            continue
        for path in base.rglob("*.rs"):
            if is_test_path(path):
                continue
            files.append(path)
    return sorted(files)


def scan_file(root: Path, path: Path) -> list[DebugPrintHit]:
    hits: list[DebugPrintHit] = []
    rel = path.relative_to(root).as_posix()
    in_block_comment = False
    for idx, raw_line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        stripped = raw_line.lstrip()
        if stripped.startswith(COMMENT_PREFIXES):
            continue
        code, in_block_comment = scrub_comments_and_strings(raw_line, in_block_comment)
        match = MACRO_RE.search(code)
        if not match:
            continue
        hits.append(
            DebugPrintHit(
                path=rel,
                line=idx,
                macro=f"{match.group(1)}!",
                text=raw_line.strip(),
            )
        )
    return hits


def scan(root: Path, scan_dirs: tuple[str, ...]) -> list[DebugPrintHit]:
    hits: list[DebugPrintHit] = []
    for path in iter_rust_files(root, scan_dirs):
        hits.extend(scan_file(root, path))
    return hits


def summary(hits: list[DebugPrintHit]) -> dict[str, object]:
    by_macro: dict[str, int] = {}
    by_path: dict[str, int] = {}
    for hit in hits:
        by_macro[hit.macro] = by_macro.get(hit.macro, 0) + 1
        by_path[hit.path] = by_path.get(hit.path, 0) + 1
    return {
        "total": len(hits),
        "by_macro": dict(sorted(by_macro.items())),
        "by_path": dict(sorted(by_path.items())),
    }


def print_text(hits: list[DebugPrintHit]) -> None:
    data = summary(hits)
    print("debug print macro report")
    print(f"total: {data['total']}")
    for macro, count in data["by_macro"].items():
        print(f"{macro}: {count}")
    if not hits:
        return
    print()
    for hit in hits:
        print(f"{hit.path}:{hit.line}: {hit.macro}: {hit.text}")


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Report println!, eprintln!, and dbg! in compiler-internal Rust sources."
    )
    parser.add_argument(
        "--root",
        type=Path,
        default=REPO_ROOT,
        help="Repository root to scan (default: current script's repository).",
    )
    parser.add_argument(
        "--scan-dir",
        action="append",
        default=None,
        help="Repository-relative directory to scan. May be repeated.",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Print machine-readable JSON instead of text.",
    )
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    root = args.root.resolve()
    scan_dirs = tuple(args.scan_dir or DEFAULT_SCAN_DIRS)
    hits = scan(root, scan_dirs)
    if args.json:
        payload = {
            "scan_dirs": list(scan_dirs),
            "summary": summary(hits),
            "hits": [asdict(hit) for hit in hits],
        }
        print(json.dumps(payload, indent=2, sort_keys=True))
    else:
        print_text(hits)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
