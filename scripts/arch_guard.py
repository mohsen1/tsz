#!/usr/bin/env python3
import pathlib
import re
import argparse
import json
import sys
from pathlib import Path

ROOT = pathlib.Path(__file__).resolve().parents[1]

CHECKS = [
    (
        "Root boundary: no tsz_solver module re-export alias",
        ROOT / "src",
        re.compile(r"\bpub\s+use\s+tsz_solver\s+as\s+solver\s*;"),
        {},
    ),
    (
        "Root boundary: no direct TypeKey internal usage in production code",
        ROOT / "src",
        re.compile(r"\btsz_solver::TypeKey\b|\btsz_solver::types::TypeKey\b|\bTypeKey::"),
        {"exclude_dirs": {"tests"}, "ignore_comment_lines": True},
    ),
    (
        "Checker boundary: direct lookup() outside query boundaries/tests",
        ROOT / "crates" / "tsz-checker",
        re.compile(r"\.lookup\s*\("),
        {"exclude_dirs": {"query_boundaries", "tests"}},
    ),
    (
        "Checker legacy surface must stay removed",
        ROOT / "crates" / "tsz-checker" / "src",
        re.compile(
            r"\bmod\s+types\s*;"
            r"|\bpub\s+mod\s+types\s*;"
            r"|\bpub\s+mod\s+arena\s*;"
            r"|\bpub\s+use\s+arena::TypeArena\b"
        ),
        {"exclude_dirs": {"tests"}},
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
        re.compile(
            r"\buse\s+tsz_solver::.*TypeKey"
            r"|\bintern\(\s*TypeKey::"
            r"|\bintern\(\s*tsz_solver::TypeKey::"
            r"|\bTypeKey::"
        ),
        {"exclude_dirs": {"tests"}, "ignore_comment_lines": True},
    ),
    (
        "Checker boundary: direct solver internal imports",
        ROOT / "crates" / "tsz-checker",
        re.compile(r"\btsz_solver::types::"),
        {"exclude_dirs": {"tests"}},
    ),
    (
        "Checker boundary: direct solver relation queries outside query boundaries/tests",
        ROOT / "crates" / "tsz-checker",
        re.compile(r"\btsz_solver::(is_subtype_of|is_assignable_to)\s*\("),
        {"exclude_dirs": {"query_boundaries", "tests"}, "ignore_comment_lines": True},
    ),
    (
        "Checker boundary: direct CallEvaluator usage outside query boundaries/tests",
        ROOT / "crates" / "tsz-checker",
        re.compile(r"\btsz_solver::CallEvaluator\b|\bCallEvaluator::new\s*\("),
        {"exclude_dirs": {"query_boundaries", "tests"}, "ignore_comment_lines": True},
    ),
    (
        "Checker boundary: direct CompatChecker construction outside query boundaries/tests",
        ROOT / "crates" / "tsz-checker",
        re.compile(r"\bCompatChecker::new\s*\(|\bCompatChecker::with_resolver\s*\("),
        {"exclude_dirs": {"query_boundaries", "tests"}, "ignore_comment_lines": True},
    ),
    (
        "Checker query boundary: call_checker must not construct CompatChecker directly",
        ROOT / "crates" / "tsz-checker" / "src" / "query_boundaries",
        re.compile(r"\bCompatChecker::with_resolver\s*\("),
        {
            "exclude_files": {
                "crates/tsz-checker/src/query_boundaries/assignability.rs",
            },
            "ignore_comment_lines": True,
        },
    ),
    (
        "Checker query boundary: call_checker must not use concrete CallEvaluator<CompatChecker>",
        ROOT / "crates" / "tsz-checker" / "src" / "query_boundaries",
        re.compile(r"\bCallEvaluator::<\s*tsz_solver::CompatChecker\s*>::"),
        {"ignore_comment_lines": True},
    ),
    (
        "Checker boundary: raw interner access",
        ROOT / "crates" / "tsz-checker",
        re.compile(r"\.intern\s*\("),
        {"exclude_dirs": {"tests"}},
    ),
    (
        "Checker boundary: deprecated two-arg intersection/union constructors",
        ROOT / "crates" / "tsz-checker",
        re.compile(r"\.intersection2\s*\(|\.union2\s*\("),
        {"exclude_dirs": {"tests"}},
    ),
    (
        "Solver dependency direction freeze",
        ROOT / "crates" / "tsz-solver",
        re.compile(r"\btsz_parser::\b|\btsz_checker::\b"),
        {"exclude_dirs": {"tests"}},
    ),
    (
        "Binder dependency direction freeze",
        ROOT / "crates" / "tsz-binder",
        re.compile(r"\btsz_solver::\b"),
        {"exclude_dirs": {"tests"}},
    ),
    (
        "Emitter dependency direction freeze",
        ROOT / "crates" / "tsz-emitter",
        re.compile(r"\btsz_checker::\b"),
        {"exclude_dirs": {"tests"}},
    ),
    (
        "Emitter boundary: direct TypeKey import/match",
        ROOT / "crates" / "tsz-emitter",
        re.compile(r"\bTypeKey::|\buse\s+tsz_solver::.*TypeKey"),
        {"exclude_dirs": {"tests"}, "ignore_comment_lines": True},
    ),
    (
        "Emitter boundary: direct lookup() on solver interner",
        ROOT / "crates" / "tsz-emitter",
        re.compile(r"\.lookup\s*\("),
        {"exclude_dirs": {"tests"}},
    ),
    (
        "Non-solver crates must not depend on TypeKey internals",
        ROOT / "crates",
        re.compile(r"\buse\s+tsz_solver::.*TypeKey|\bTypeKey::"),
        {"exclude_dirs": {"tsz-solver", "tests"}, "ignore_comment_lines": True},
    ),
    # --- WASM compatibility rules ---
    # Crates compiled to WASM: all except tsz-cli and conformance.
    # std::time::Instant panics at runtime on wasm32-unknown-unknown (no clock);
    # use web_time::Instant which is a drop-in replacement on all platforms.
    (
        "WASM compat: std::time::Instant banned in WASM-compiled crates (use web_time::Instant)",
        ROOT / "crates",
        re.compile(
            r"\buse\s+std::time::Instant\b"
            r"|\buse\s+std::time::\{[^}]*\bInstant\b"
            r"|\bstd::time::Instant::"
        ),
        {
            "exclude_dirs": {"tsz-cli", "conformance", "tests"},
            "ignore_comment_lines": True,
        },
    ),
    # std::time::SystemTime also panics on wasm32-unknown-unknown.
    (
        "WASM compat: std::time::SystemTime banned in WASM-compiled crates",
        ROOT / "crates",
        re.compile(
            r"\buse\s+std::time::SystemTime\b"
            r"|\buse\s+std::time::\{[^}]*\bSystemTime\b"
            r"|\bstd::time::SystemTime::"
        ),
        {
            "exclude_dirs": {"tsz-cli", "conformance", "tests"},
            "ignore_comment_lines": True,
        },
    ),
    (
        "Non-solver/non-lowering crates must not inspect TypeData internals in production code",
        ROOT / "crates",
        re.compile(r"\buse\s+tsz_solver::.*TypeData\b|\bTypeData::"),
        {
            "exclude_dirs": {"tsz-solver", "tsz-lowering", "tests"},
            "ignore_comment_lines": True,
        },
    ),
    (
        "LSP boundary: direct lookup() on solver interner",
        ROOT / "crates" / "tsz-lsp",
        re.compile(r"\.lookup\s*\("),
        {"exclude_dirs": {"tests"}},
    ),
    (
        "Checker test boundary: no direct solver internal type inspection in integration tests",
        ROOT / "crates" / "tsz-checker" / "tests",
        re.compile(r"\btsz_solver::types::|\bTypeData::|\buse\s+tsz_solver::TypeData\b"),
        {"exclude_files": {"crates/tsz-checker/tests/architecture_contract_tests.rs"}},
    ),
    (
        "Checker test boundary: no direct solver internal type inspection in src tests",
        ROOT / "crates" / "tsz-checker" / "src" / "tests",
        re.compile(r"\btsz_solver::types::|\bTypeData::|\buse\s+tsz_solver::TypeData\b"),
        {
            "exclude_files": {
                "crates/tsz-checker/src/tests/architecture_contract_tests.rs",
            }
        },
    ),
]

MANIFEST_CHECKS = [
    (
        "Emitter manifest dependency freeze",
        ROOT / "crates" / "tsz-emitter" / "Cargo.toml",
        re.compile(r"^\s*tsz-checker\s*=", re.MULTILINE),
    ),
    (
        "Binder manifest dependency freeze",
        ROOT / "crates" / "tsz-binder" / "Cargo.toml",
        re.compile(r"^\s*tsz-solver\s*=", re.MULTILINE),
    ),
    (
        "Checker manifest: legacy type arena feature must stay removed",
        ROOT / "crates" / "tsz-checker" / "Cargo.toml",
        re.compile(r"^\s*legacy-type-arena\s*=", re.MULTILINE),
    ),
]

LINE_LIMIT_CHECKS = [
    (
        "Checker boundary: src files must stay under 2000 LOC",
        ROOT / "crates" / "tsz-checker" / "src",
        2000,
    ),
]

EXCLUDE_DIRS = {".git", "target", "node_modules"}
SOLVER_TYPEDATA_QUARANTINE_ALLOWLIST = {
    "crates/tsz-solver/src/intern.rs",
    "crates/tsz-solver/src/intern_intersection.rs",
    "crates/tsz-solver/src/intern_normalize.rs",
    "crates/tsz-solver/src/intern_template.rs",
}


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
        if excludes.get("ignore_comment_lines", False):
            if line.lstrip().startswith("//"):
                continue
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


def scan_line_limits(base: pathlib.Path, limit: int):
    hits = []
    for path, rel in iter_rs_files(base):
        line_count = 0
        try:
            with path.open("r", encoding="utf-8", errors="ignore") as handle:
                for line_count, _line in enumerate(handle, start=1):
                    pass
        except OSError:
            continue
        if line_count > limit:
            hits.append(f"{rel}:{line_count} lines (limit {limit})")
    return hits


def strip_rust_comments(text: str) -> str:
    chars = list(text)
    i = 0
    n = len(chars)
    out = []
    state = "code"
    block_depth = 0
    raw_hash_count = 0

    while i < n:
        ch = chars[i]
        nxt = chars[i + 1] if i + 1 < n else ""

        if state == "line_comment":
            if ch == "\n":
                out.append("\n")
                state = "code"
            else:
                out.append(" ")
            i += 1
            continue

        if state == "block_comment":
            if ch == "/" and nxt == "*":
                block_depth += 1
                out.extend([" ", " "])
                i += 2
                continue
            if ch == "*" and nxt == "/":
                block_depth -= 1
                out.extend([" ", " "])
                i += 2
                if block_depth == 0:
                    state = "code"
                continue
            out.append("\n" if ch == "\n" else " ")
            i += 1
            continue

        if state == "string":
            out.append(ch)
            if ch == "\\" and i + 1 < n:
                out.append(chars[i + 1])
                i += 2
                continue
            if ch == '"':
                state = "code"
            i += 1
            continue

        if state == "char":
            out.append(ch)
            if ch == "\\" and i + 1 < n:
                out.append(chars[i + 1])
                i += 2
                continue
            if ch == "'":
                state = "code"
            i += 1
            continue

        if state == "raw_string":
            out.append(ch)
            if ch == '"' and raw_hash_count == 0:
                state = "code"
                i += 1
                continue
            if ch == '"' and raw_hash_count > 0:
                hashes = 0
                j = i + 1
                while j < n and chars[j] == "#" and hashes < raw_hash_count:
                    hashes += 1
                    j += 1
                if hashes == raw_hash_count:
                    out.extend(["#"] * hashes)
                    i = j
                    state = "code"
                    continue
            i += 1
            continue

        if ch == "/" and nxt == "/":
            out.extend([" ", " "])
            i += 2
            state = "line_comment"
            continue
        if ch == "/" and nxt == "*":
            out.extend([" ", " "])
            i += 2
            state = "block_comment"
            block_depth = 1
            continue
        if ch == '"':
            out.append(ch)
            i += 1
            state = "string"
            continue
        if ch == "'":
            out.append(ch)
            i += 1
            state = "char"
            continue
        if ch == "r":
            j = i + 1
            hashes = 0
            while j < n and chars[j] == "#":
                hashes += 1
                j += 1
            if j < n and chars[j] == '"':
                out.append("r")
                out.extend(["#"] * hashes)
                out.append('"')
                i = j + 1
                state = "raw_string"
                raw_hash_count = hashes
                continue

        out.append(ch)
        i += 1

    return "".join(out)


def scan_solver_typedata_quarantine(base: pathlib.Path):
    hits = set()
    alias_re = re.compile(r"\bTypeData\s+as\s+([A-Za-z_]\w*)\b")
    type_alias_re = re.compile(r"\btype\s+([A-Za-z_]\w*)\s*=\s*[^;]*\bTypeData\b[^;]*;")
    direct_intern_re = re.compile(
        r"\.intern\s*\(\s*(?:crate::types::TypeData|tsz_solver::TypeData|TypeData)\s*::",
        re.MULTILINE,
    )

    for path, rel in iter_rs_files(base):
        if "/tests/" in rel or any(rel.endswith(allow) for allow in SOLVER_TYPEDATA_QUARANTINE_ALLOWLIST):
            continue

        try:
            text = path.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        text_without_comments = strip_rust_comments(text)

        aliases = {"TypeData"}
        for alias_match in alias_re.finditer(text_without_comments):
            aliases.add(alias_match.group(1))
        for statement in text_without_comments.split(";"):
            normalized = " ".join(statement.split())
            type_alias_match = type_alias_re.search(f"{normalized};")
            if type_alias_match:
                aliases.add(type_alias_match.group(1))

        for match in direct_intern_re.finditer(text_without_comments):
            line_idx = text_without_comments.count("\n", 0, match.start())
            hits.add(f"{rel}:{line_idx + 1}")

        for alias in aliases:
            if alias == "TypeData":
                continue
            alias_re_intern = re.compile(
                rf"\.intern\s*\(\s*{re.escape(alias)}\s*::",
                re.MULTILINE,
            )
            for match in alias_re_intern.finditer(text_without_comments):
                line_idx = text_without_comments.count("\n", 0, match.start())
                hits.add(f"{rel}:{line_idx + 1}")

    return sorted(hits)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run TSZ architecture guardrails"
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Emit machine-readable output instead of human-readable diagnostics.",
    )
    parser.add_argument(
        "--json-report",
        metavar="PATH",
        default="",
        help="Write machine-readable report to this path (still exits non-zero on failures).",
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

    for name, manifest_path, pattern in MANIFEST_CHECKS:
        if not manifest_path.exists():
            continue
        text = manifest_path.read_text(encoding="utf-8", errors="ignore")
        hits = []
        for i, line in enumerate(text.splitlines(), start=1):
            if pattern.search(line):
                rel = manifest_path.relative_to(ROOT).as_posix()
                hits.append(f"{rel}:{i}")
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, base, limit in LINE_LIMIT_CHECKS:
        if not base.exists():
            continue
        hits = scan_line_limits(base, limit)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    solver_typedata_hits = scan_solver_typedata_quarantine(ROOT / "crates" / "tsz-solver")
    total_hits += len(solver_typedata_hits)
    if solver_typedata_hits:
        failures.append(
            (
                "Solver TypeData construction must stay in interner files",
                solver_typedata_hits,
            )
        )

    payload = {
        "status": "failed" if failures else "passed",
        "total_hits": total_hits,
        "failures": [{"name": name, "hits": hits} for name, hits in failures],
    }

    if args.json_report:
        report_path = Path(args.json_report)
        if report_path.parent != Path("."):
            report_path.parent.mkdir(parents=True, exist_ok=True)
        report_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")

    if args.json:
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
