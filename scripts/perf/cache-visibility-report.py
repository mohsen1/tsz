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
CACHE_NAME_PATTERN = (
    r"(?:[A-Za-z_][A-Za-z0-9_]*(?:cache|Cache|memo|Memo)[A-Za-z0-9_]*"
    r"|cache|Cache|memo|Memo)"
)

FIELD_RE = re.compile(
    r"^\s*(?:(?:pub|pub\(crate\)|pub\(super\)|pub\(in [^)]+\))\s+)?"
    rf"(?P<name>{CACHE_NAME_PATTERN})"
    r"\s*:\s*(?P<type>.+)$"
)
TYPE_ALIAS_RE = re.compile(
    r"^\s*(?:(?:pub|pub\(crate\)|pub\(super\)|pub\(in [^)]+\))\s+)?"
    rf"type\s+(?P<name>{CACHE_NAME_PATTERN})"
    r"\s*=\s*(?P<type>.+)$"
)
STRUCT_RE = re.compile(
    r"^\s*(?:(?:pub|pub\(crate\)|pub\(super\)|pub\(in [^)]+\))\s+)?"
    r"struct\s+([A-Za-z_][A-Za-z0-9_]*)\b"
)
STATS_RE = re.compile(r"(?:hits?|miss(?:es)?|entries|entry_count|Stats|statistics|total_entries)")
SIZE_RE = re.compile(r"(?:estimated_size_bytes|size_bytes|memory|Memory|total_entries|entries)")

RETAINED_OWNERS = {
    "BinderState",
    "CheckerContext",
    "LibLoader",
    "ModuleResolver",
    "QueryCache",
    "TypeCache",
    "TypeInterner",
}
OPERATION_LOCAL_OWNERS = {
    "ApplicationEvaluator",
    "CallEvaluator",
    "Canonicalizer",
    "CompatChecker",
    "ContainsTypeChecker",
    "DefaultJudge",
    "FlowAnalyzer",
    "FreeInferChecker",
    "FreeTypeParamChecker",
    "InferenceContext",
    "ShallowContainsTypeChecker",
    "SubtypeChecker",
    "TypeEvaluator",
    "TypeFormatter",
}
RETAINED_MODULE_PATHS = {
    "crates/tsz-checker/src/context/aliases.rs",
    "crates/tsz-checker/src/flow/control_flow/core.rs",
    "crates/tsz-lsp/src/resolver/core.rs",
}
SNAPSHOT_OWNERS = {"CacheSnapshot"}


@dataclass(frozen=True)
class CacheCandidate:
    path: str
    line: int
    owner: str
    name: str
    type: str
    retention: str
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


def scrub_comments_and_strings(line: str, in_block_comment: bool) -> tuple[str, bool]:
    """Return code with comments and string contents removed."""
    escaped = False
    in_string = False
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
        if ch == "\\" and in_string:
            chars.append(" ")
            escaped = True
            i += 1
            continue
        if ch == '"':
            in_string = not in_string
            chars.append(" ")
            i += 1
            continue
        if ch == "/" and nxt == "*" and not in_string:
            in_block_comment = True
            i += 2
            continue
        if ch == "/" and nxt == "/" and not in_string:
            break
        chars.append(" " if in_string else ch)
        i += 1
    return "".join(chars), in_block_comment


def trim_type(raw: str) -> str:
    return raw.strip().rstrip(",;").strip()


def is_cache_type(type_text: str) -> bool:
    return any(map_type in type_text for map_type in MAP_TYPES)


def stat_stem(name: str) -> str:
    stem = re.sub(r"(?:_?cache|_?memo)$", "", name, flags=re.IGNORECASE)
    return stem or name


def camel_to_snake(name: str) -> str:
    with_boundaries = re.sub(r"(.)([A-Z][a-z]+)", r"\1_\2", name)
    with_boundaries = re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", with_boundaries)
    return with_boundaries.lower()


def stat_stems(name: str) -> tuple[str, ...]:
    stem = stat_stem(name)
    snake_stem = camel_to_snake(stem)
    if snake_stem == stem:
        return (stem,)
    return (stem, snake_stem)


def has_stats_signal(file_text: str, name: str) -> bool:
    name_re = re.escape(name)
    patterns = [rf"{name_re}\.len\s*\("]
    for stem in stat_stems(name):
        stem_re = re.escape(stem)
        patterns.extend(
            (
                rf"{stem_re}.*(?:hits?|miss(?:es)?|entries)",
                rf"(?:hits?|miss(?:es)?|entries).*{stem_re}",
            )
        )
    return any(re.search(pattern, file_text, re.IGNORECASE) for pattern in patterns)


def has_size_signal(file_text: str, name: str) -> bool:
    name_re = re.escape(name)
    patterns = [rf"{name_re}\.len\s*\("]
    for stem in stat_stems(name):
        stem_re = re.escape(stem)
        patterns.extend(
            (
                rf"{stem_re}.*(?:estimated_size_bytes|size_bytes|memory|total_entries|entries)",
                rf"(?:estimated_size_bytes|size_bytes|memory|total_entries|entries).*{stem_re}",
            )
        )
    return any(re.search(pattern, file_text, re.IGNORECASE) for pattern in patterns)


def classify_retention(path: str, owner: str) -> str:
    if owner in SNAPSHOT_OWNERS:
        return "snapshot"
    if owner in RETAINED_OWNERS:
        return "retained"
    if owner in OPERATION_LOCAL_OWNERS:
        return "operation_local"
    if owner == "<module>" and path in RETAINED_MODULE_PATHS:
        return "retained"
    if owner == "<module>":
        return "module"
    return "unknown"


def scan_file(path: Path) -> list[CacheCandidate]:
    raw_text = path.read_text(encoding="utf-8")
    code_lines: list[str] = []
    in_block_comment = False
    for raw_line in raw_text.splitlines():
        code, in_block_comment = scrub_comments_and_strings(raw_line, in_block_comment)
        code_lines.append(code)
    file_text = "\n".join(code_lines)
    rel = relative(path)
    owner = "<module>"
    struct_depth = 0
    candidates: list[CacheCandidate] = []
    for line_no, line in enumerate(code_lines, start=1):
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
                retention=classify_retention(rel, owner),
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
    by_retention: dict[str, int] = {}
    for candidate in candidates:
        by_retention[candidate.retention] = by_retention.get(candidate.retention, 0) + 1
        if candidate.needs_review:
            by_path[candidate.path] = by_path.get(candidate.path, 0) + 1
            by_owner[candidate.owner] = by_owner.get(candidate.owner, 0) + 1
    return {
        "schema_version": 2,
        "total_candidates": len(candidates),
        "needs_review": sum(1 for candidate in candidates if candidate.needs_review),
        "with_stats_signal": sum(1 for candidate in candidates if candidate.stats_signal),
        "with_size_signal": sum(1 for candidate in candidates if candidate.size_signal),
        "candidates_by_retention": dict(sorted(by_retention.items())),
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
    print(f"  candidates_by_retention: {summary['candidates_by_retention']}")
    if not candidates:
        return
    print()
    print("candidates:")
    for candidate in candidates:
        marker = "review" if candidate.needs_review else "covered"
        print(
            f"  {marker} {candidate.path}:{candidate.line} "
            f"{candidate.owner}.{candidate.name} "
            f"retention={candidate.retention} "
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
    parser.add_argument(
        "--retained-only",
        action="store_true",
        help="only report candidates classified as retained residency surfaces",
    )
    args = parser.parse_args(argv)

    roots = args.root if args.root is not None else [ROOT / root for root in DEFAULT_ROOTS]
    candidates = scan(roots)
    if args.retained_only:
        candidates = [candidate for candidate in candidates if candidate.retention == "retained"]
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
