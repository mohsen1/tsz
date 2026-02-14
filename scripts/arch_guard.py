#!/usr/bin/env python3
import pathlib
import re
import argparse
import json
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]

CHECKS = [
    (
        "Checker boundary: direct lookup() outside query boundaries/tests",
        ROOT / "crates" / "tsz-checker",
        re.compile(r"\.lookup\s*\("),
        {"exclude_dirs": {"query_boundaries", "tests"}},
    ),
    (
        "Checker boundary: direct TypeKey inspection outside query boundaries/tests",
        ROOT / "crates" / "tsz-checker",
        re.compile(r"^\s*(match|if let|if matches!|matches!\().*TypeKey::"),
        {"exclude_dirs": {"query_boundaries", "tests"}},
    ),
    (
        "Checker boundary: direct TypeKey import/intern usage",
        ROOT / "crates" / "tsz-checker",
        re.compile(r"\buse\s+tsz_solver::.*TypeKey|\bintern\(\s*TypeKey::"),
        {"exclude_dirs": {"tests"}},
    ),
    (
        "Checker boundary: direct solver internal imports",
        ROOT / "crates" / "tsz-checker",
        re.compile(r"\btsz_solver::types::"),
        {"exclude_dirs": {"tests"}},
    ),
    (
        "Checker boundary: raw interner access",
        ROOT / "crates" / "tsz-checker",
        re.compile(r"\.intern\s*\("),
        {"exclude_dirs": {"tests"}},
    ),
    (
        "Solver dependency direction freeze",
        ROOT / "crates" / "tsz-solver",
        re.compile(r"\btsz_parser::\b|\btsz_checker::\b"),
        {"exclude_files": {"crates/tsz-solver/src/lower.rs"}},
    ),
]

EXCLUDE_DIRS = {".git", "target", "node_modules"}
def iter_rs_files(base: pathlib.Path):
    for path in base.rglob("*.rs"):
        rel = path.relative_to(ROOT).as_posix()
        parts = set(rel.split("/"))
        if EXCLUDE_DIRS.intersection(parts):
            continue
        yield path, rel


def find_matches(file_text: str, pattern: re.Pattern[str], rel: str, excludes: dict):
    matches = []
    excluded_files = set(excludes.get("exclude_files", ()))
    if rel in excluded_files:
        return matches

    exclude_dirs = set(excludes.get("exclude_dirs", ()))
    part_set = set(rel.split("/"))
    if exclude_dirs and exclude_dirs.intersection(part_set):
        return matches

    for i, line in enumerate(file_text.splitlines(), start=1):
        if pattern.search(line):
            matches.append(i)
    return matches


def scan(base, pattern, excludes):
    hits = []
    for path, rel in iter_rs_files(base):
        try:
            text = path.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        for ln in find_matches(text, pattern, rel, excludes):
            hits.append(f"{rel}:{ln}")
    return hits


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run TSZ architecture guardrails"
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Emit machine-readable output instead of human-readable diagnostics.",
    )
    args = parser.parse_args()

    failures = []
    total_hits = 0
    for name, base, pattern, excludes in CHECKS:
        if not base.exists():
            continue
        hits = scan(base, pattern, excludes)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    if args.json:
        payload = {
            "status": "failed" if failures else "passed",
            "total_hits": total_hits,
            "failures": [{"name": name, "hits": hits} for name, hits in failures],
        }
        print(json.dumps(payload, indent=2))
        return 0 if not failures else 1

    if failures:
        print("ARCH GUARD FAILURES:")
        for name, hits in failures:
            print(f"- {name}:")
            for hit in hits[:200]:
                print(f"  - {hit}")
            if len(hits) > 200:
                extra = len(hits) - 200
                print(f"  - ... and {extra} more")
        return 1

    print("Architecture guardrails passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
