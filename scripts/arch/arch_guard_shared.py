import pathlib
import re
import argparse
import importlib.util
import json
import shlex
import subprocess
import sys
from collections import Counter
from pathlib import Path
from typing import BinaryIO, Iterable, Optional

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - exercised on Python < 3.11.
    tomllib = None

ROOT = pathlib.Path(__file__).resolve().parents[2]
POLICY_PATH = pathlib.Path(__file__).resolve().parent / "arch_guard_policy.toml"


def _strip_toml_comment(line: str) -> str:
    in_basic = False
    in_literal = False
    escaped = False

    for index, char in enumerate(line):
        if in_basic:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_basic = False
            continue
        if in_literal:
            if char == "'":
                in_literal = False
            continue
        if char == '"':
            in_basic = True
        elif char == "'":
            in_literal = True
        elif char == "#":
            return line[:index]
    return line


def _parse_toml_string(value: str) -> str:
    value = value.strip()
    if value.startswith("'") and value.endswith("'"):
        return value[1:-1]
    if value.startswith('"') and value.endswith('"'):
        return json.loads(value)
    raise ValueError(f"unsupported TOML string value: {value!r}")


def _parse_toml_string_array(lines: list[str]) -> list[str]:
    text = "\n".join(_strip_toml_comment(line) for line in lines)
    start = text.find("[")
    end = text.rfind("]")
    if start == -1 or end == -1 or end < start:
        raise ValueError(f"unsupported TOML array value: {text!r}")

    items: list[str] = []
    index = start + 1
    while index < end:
        while index < end and text[index] in " \t\r\n,":
            index += 1
        if index >= end:
            break

        quote = text[index]
        if quote not in {"'", '"'}:
            raise ValueError(f"unsupported TOML array item near: {text[index:end]!r}")
        index += 1
        item_start = index
        if quote == "'":
            while index < end and text[index] != "'":
                index += 1
            if index >= end:
                raise ValueError("unterminated TOML literal string in array")
            items.append(text[item_start:index])
            index += 1
            continue

        escaped = False
        buffer: list[str] = []
        while index < end:
            char = text[index]
            if escaped:
                buffer.append("\\" + char)
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                break
            else:
                buffer.append(char)
            index += 1
        if index >= end:
            raise ValueError("unterminated TOML basic string in array")
        items.append(json.loads(f'"{"".join(buffer)}"'))
        index += 1

    return items


def _parse_toml_value(value: str, array_lines: list[str]) -> object:
    value = _strip_toml_comment(value).strip()
    if value == "true":
        return True
    if value == "false":
        return False
    if value.startswith("["):
        return _parse_toml_string_array(array_lines)
    return _parse_toml_string(value)


def _parse_arch_guard_policy_toml(text: str) -> dict:
    """Parse the arch guard policy subset when stdlib `tomllib` is unavailable."""
    data: dict[str, list[dict]] = {"pattern_checks": [], "manifest_checks": []}
    current: Optional[dict] = None
    lines = text.splitlines()
    index = 0

    while index < len(lines):
        raw_line = lines[index]
        line = _strip_toml_comment(raw_line).strip()
        index += 1

        if not line:
            continue
        if line.startswith("[[") and line.endswith("]]"):
            table_name = line[2:-2].strip()
            if table_name not in data:
                raise ValueError(f"unsupported TOML table: {table_name!r}")
            current = {}
            data[table_name].append(current)
            continue
        if current is None or "=" not in line:
            raise ValueError(f"unsupported TOML line: {raw_line!r}")

        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip()
        array_lines = [raw_line]
        if value.startswith("[") and "]" not in _strip_toml_comment(value):
            while index < len(lines):
                array_lines.append(lines[index])
                if "]" in _strip_toml_comment(lines[index]):
                    index += 1
                    break
                index += 1
        current[key] = _parse_toml_value(value, array_lines)

    return data


def _load_policy_toml(file: BinaryIO) -> dict:
    if tomllib is not None:
        return tomllib.load(file)
    return _parse_arch_guard_policy_toml(file.read().decode("utf-8"))


def _build_excludes(entry: dict) -> dict:
    excludes: dict = {}
    if entry.get("exclude_dirs") is not None:
        excludes["exclude_dirs"] = set(entry["exclude_dirs"])
    if entry.get("exclude_files") is not None:
        excludes["exclude_files"] = set(entry["exclude_files"])
    if entry.get("exclude_test_files"):
        excludes["exclude_test_files"] = True
    if entry.get("ignore_comment_lines"):
        excludes["ignore_comment_lines"] = True
    return excludes


def _parse_pattern_checks(data: dict) -> list[tuple[str, pathlib.Path, re.Pattern, dict]]:
    return [
        (entry["name"], ROOT / entry["base"], re.compile(entry["pattern"]), _build_excludes(entry))
        for entry in data.get("pattern_checks", [])
    ]


def _parse_manifest_checks(data: dict) -> list[tuple[str, pathlib.Path, re.Pattern]]:
    return [
        (entry["name"], ROOT / entry["file"], re.compile(entry["pattern"], re.MULTILINE))
        for entry in data.get("manifest_checks", [])
    ]


def _load_pattern_checks(
    policy_path: pathlib.Path = POLICY_PATH,
) -> list[tuple[str, pathlib.Path, re.Pattern, dict]]:
    """Load [[pattern_checks]] entries from the declarative policy TOML."""
    with policy_path.open("rb") as f:
        return _parse_pattern_checks(_load_policy_toml(f))


def _load_manifest_checks(
    policy_path: pathlib.Path = POLICY_PATH,
) -> list[tuple[str, pathlib.Path, re.Pattern]]:
    """Load [[manifest_checks]] entries from the declarative policy TOML.

    Patterns are compiled with ``re.MULTILINE`` so ``^`` and ``$`` match
    at line boundaries within Cargo.toml files.
    """
    with policy_path.open("rb") as f:
        return _parse_manifest_checks(_load_policy_toml(f))


def _load_all_checks(
    policy_path: pathlib.Path = POLICY_PATH,
) -> tuple[list[tuple[str, pathlib.Path, re.Pattern, dict]], list[tuple[str, pathlib.Path, re.Pattern]]]:
    """Parse the policy TOML once and return both check lists."""
    with policy_path.open("rb") as f:
        data = _load_policy_toml(f)
    return _parse_pattern_checks(data), _parse_manifest_checks(data)


CHECKS, MANIFEST_CHECKS = _load_all_checks()

# Repo-wide physical-line cap. `.claude/CLAUDE.md` states the 2000-LOC rule
# without qualification ("No hand-authored source ... shard may exceed 2000
# physical lines"), but for a long time only `tsz-checker/src` was registered
# here, so every other crate drifted past the cap while `arch-size` stayed
# green (#16733). The list below now registers one directory-level check per
# `crates/*/src` root, so a *new* file crossing 2000 lines in any crate fails
# the size gate — not just files that already happen to carry a per-file
# ratchet in `FILE_LINE_LIMIT_CHECKS`.
#
# `scan_line_limit_coverage` (below) plus its unit test assert that the set of
# registered roots equals the set of `crates/*/src` roots on disk, so a newly
# added crate cannot silently reappear as an unguarded blind spot.
#
# Each crate's fourth tuple element is its audited allowlist of files already
# over 2000 lines when repo-wide coverage landed. These are grandfathered
# debt: `test_excluded_files_actually_exceed_limit` keeps every entry honest
# (must exist and actually exceed the cap), and `FILE_LINE_LIMIT_CHECKS` pins
# the ones that also need an exact-size ratchet. Delete an entry in the same
# diff that splits the file below the cap; the allowlist can only shrink.
SRC_LINE_LIMIT = 2000

# One row per `crates/*/src` root: (crate directory, display label, audited
# allowlist of files already over the cap when repo-wide coverage landed). The
# check name is uniformly "<label> boundary: src files must stay under 2000
# LOC" — for "Checker" that reproduces the historical string several unit tests
# key off, so no special case is needed. tsz-checker's allowlist stays empty:
# this guard drove it to full compliance and holds it there, which is the proof
# the mechanism works.
_CRATE_SRC_LINE_LIMIT_ALLOWLISTS = [
    ("tsz-checker", "Checker", set()),
    # `tsz-binder` and `tsz-cli` carry no grandfathered debt, for the same
    # reason `tsz-checker` does not: the splits in this change drove both crates
    # to full compliance, so every file that was over the cap when these roots
    # were registered is now under it —
    #   crates/tsz-binder/src/state/core.rs             2006 -> 1744
    #   crates/tsz-cli/src/bin/tsz_server/main.rs       2038 -> 1625
    #   crates/tsz-cli/src/driver/check_utils/tests.rs  2258 -> 1152
    # Leaving the entries in place fails `test_excluded_files_actually_exceed_limit`,
    # which is exactly the honesty check working.
    ("tsz-binder", "Binder", set()),
    ("tsz-cli", "CLI", set()),
    ("tsz-common", "Common", {
        "crates/tsz-common/src/perf_counters/tests.rs",
    }),
    ("tsz-core", "Core", {
        "crates/tsz-core/src/config/mod.rs",
        "crates/tsz-core/src/config/tests/module_resolution.rs",
    }),
    ("tsz-emitter", "Emitter", {
        "crates/tsz-emitter/src/declaration_emitter/tests/type_info.rs",
        "crates/tsz-emitter/src/emitter/functions.rs",
        "crates/tsz-emitter/src/transforms/helpers.rs",
        "crates/tsz-emitter/src/emitter/helpers.rs",
        "crates/tsz-emitter/src/emitter/expressions/call.rs",
        "crates/tsz-emitter/src/emitter/source_file/emit.rs",
        "crates/tsz-emitter/src/emitter/source_file/es5_emit_tests.rs",
        "crates/tsz-emitter/src/emitter/declarations/class/emit_es6.rs",
        "crates/tsz-emitter/src/transforms/class_es5_ir_members.rs",
        "crates/tsz-emitter/src/declaration_emitter/helpers/portability_resolve.rs",
    }),
    ("tsz-lowering", "Lowering", {
        "crates/tsz-lowering/src/lower/core.rs",
    }),
    ("tsz-lsp", "LSP", set()),
    ("tsz-parser", "Parser", {
        "crates/tsz-parser/src/parser/state_expressions_literals_regex.rs",
        "crates/tsz-parser/src/parser/state_statements_class_members.rs",
    }),
    ("tsz-scanner", "Scanner", set()),
    ("tsz-solver", "Solver", {
        "crates/tsz-solver/src/type_queries/core.rs",
        "crates/tsz-solver/src/type_queries/flow.rs",
        "crates/tsz-solver/src/operations/constraints/walker.rs",
        "crates/tsz-solver/src/relations/subtype/rules/objects.rs",
        "crates/tsz-solver/src/type_queries/data/tests.rs",
        "crates/tsz-solver/src/evaluation/evaluate_rules/conditional.rs",
        "crates/tsz-solver/src/def/core.rs",
        "crates/tsz-solver/src/intern/core/constructors.rs",
        "crates/tsz-solver/src/relations/subtype/explain.rs",
    }),
    ("tsz-wasm", "WASM", set()),
    ("tsz-website", "Website", set()),
    ("conformance", "Conformance", set()),
]

LINE_LIMIT_CHECKS = [
    (
        f"{label} boundary: src files must stay under 2000 LOC",
        ROOT / "crates" / crate / "src",
        SRC_LINE_LIMIT,
        allowlist,
    )
    for crate, label, allowlist in _CRATE_SRC_LINE_LIMIT_ALLOWLISTS
] + [
    (
        "Checker computation boundary: type-computation monoliths must stay below 3100 LOC (#8226)",
        ROOT / "crates" / "tsz-checker" / "src" / "types" / "computation",
        3100,
    ),
    (
        "Binder boundary: src files must stay under 2000 LOC (#16733)",
        ROOT / "crates" / "tsz-binder" / "src",
        2000,
        # (empty — all files are under the 2000-line limit; #16733 split
        # `state/core.rs` into `state/core.rs` + `state/core_incremental.rs`
        # to bring the crate into compliance)
        set(),
    ),
    (
        "CLI boundary: src files must stay under 2000 LOC (#16733)",
        ROOT / "crates" / "tsz-cli" / "src",
        2000,
        # (empty — all files are under the 2000-line limit; #16733 split
        # `bin/tsz_server/main.rs` into `bracket_match.rs` and
        # `driver/check_utils/tests.rs` into `tests_part2.rs` to bring the
        # crate into compliance)
        set(),
    ),
]


LINE_LIMIT_COVERAGE_NAME = (
    "Architecture boundary: every crates/*/src root must be registered in the "
    "2000-LOC LINE_LIMIT_CHECKS (repo-wide coverage cannot silently diverge; #16733)"
)


def crate_src_roots(crates_dir: Optional[pathlib.Path] = None) -> list[pathlib.Path]:
    """Every ``crates/*/src`` directory on disk, sorted for deterministic output."""
    crates_dir = crates_dir if crates_dir is not None else (ROOT / "crates")
    if not crates_dir.is_dir():
        return []
    return sorted(
        child / "src"
        for child in crates_dir.iterdir()
        if (child / "src").is_dir()
    )


def scan_line_limit_coverage(
    checks=None, crates_dir: Optional[pathlib.Path] = None
) -> list[str]:
    """Report ``crates/*/src`` roots missing a directory-level 2000-LOC check.

    The 2000-LOC cap is documented repo-wide, so every crate's ``src`` tree must
    be registered as a base in ``LINE_LIMIT_CHECKS`` at ``SRC_LINE_LIMIT``.
    Coverage gaps are invisible by construction — a path that is never scanned
    never fails — so this makes the *coverage* itself a checked invariant
    (#16733). A newly added crate that forgets to register here fails the guard.
    """
    checks = checks if checks is not None else LINE_LIMIT_CHECKS
    registered = {
        base.resolve()
        for _name, base, limit, *_rest in checks
        if limit == SRC_LINE_LIMIT
    }
    missing = []
    for src in crate_src_roots(crates_dir):
        if src.resolve() not in registered:
            missing.append(src.relative_to(ROOT).as_posix())
    return missing


# `crates/*/tests` coverage (#16745). #16733 registered every `crates/*/src`
# root; `.claude/CLAUDE.md` states the 2000-LOC cap over "source, test,
# script, or generated-code" without qualification, so the tests trees were
# still an unguarded blind spot in the same way `src` was before #16733. Same
# three-part contract: a coverage test asserts registered roots equal the
# `crates/*/tests` roots on disk, allowlists are audited (must exist and
# actually exceed the cap) and shrink-only, and this list is consulted by the
# same `scan_line_limits` walk as `LINE_LIMIT_CHECKS` — it does not need its
# own scanner, only its own coverage check.
TESTS_LINE_LIMIT = SRC_LINE_LIMIT

# One row per `crates/*/tests` root that exists on disk (tsz-wasm and
# tsz-website have no top-level `tests/` directory, so they are not listed).
_CRATE_TESTS_LINE_LIMIT_ALLOWLISTS = [
    ("conformance", "Conformance", set()),
    ("tsz-binder", "Binder", set()),
    ("tsz-checker", "Checker", {
        "crates/tsz-checker/tests/symbol_index_signature_tests.rs",
        "crates/tsz-checker/tests/ts2353_tests.rs",
    }),
    ("tsz-cli", "CLI", {
        "crates/tsz-cli/tests/driver_tests_parts/part_12.rs",
        "crates/tsz-cli/tests/tsc_compat_tests_parts/part_00.rs",
    }),
    ("tsz-common", "Common", set()),
    ("tsz-core", "Core", {
        "crates/tsz-core/tests/parser_state_tests_parts/part_00.rs",
    }),
    ("tsz-emitter", "Emitter", set()),
    ("tsz-lowering", "Lowering", set()),
    ("tsz-lsp", "LSP", {
        "crates/tsz-lsp/tests/hover_tests.rs",
    }),
    ("tsz-parser", "Parser", set()),
    ("tsz-scanner", "Scanner", set()),
    ("tsz-solver", "Solver", {
        "crates/tsz-solver/tests/canonicalize_tests.rs",
        "crates/tsz-solver/tests/intern_tests.rs",
    }),
]

TESTS_LINE_LIMIT_CHECKS = [
    (
        f"{label} tests boundary: tests files must stay under 2000 LOC (#16745)",
        ROOT / "crates" / crate / "tests",
        TESTS_LINE_LIMIT,
        allowlist,
    )
    for crate, label, allowlist in _CRATE_TESTS_LINE_LIMIT_ALLOWLISTS
]

TESTS_LINE_LIMIT_COVERAGE_NAME = (
    "Architecture boundary: every crates/*/tests root must be registered in "
    "TESTS_LINE_LIMIT_CHECKS (2000-LOC cap extends to tests dirs; #16745)"
)


def crate_tests_roots(crates_dir: Optional[pathlib.Path] = None) -> list[pathlib.Path]:
    """Every ``crates/*/tests`` directory on disk, sorted for deterministic output."""
    crates_dir = crates_dir if crates_dir is not None else (ROOT / "crates")
    if not crates_dir.is_dir():
        return []
    return sorted(
        child / "tests"
        for child in crates_dir.iterdir()
        if (child / "tests").is_dir()
    )


def scan_tests_line_limit_coverage(
    checks=None, crates_dir: Optional[pathlib.Path] = None
) -> list[str]:
    """Report ``crates/*/tests`` roots missing a directory-level 2000-LOC check.

    Mirrors ``scan_line_limit_coverage`` for the ``src`` roots (#16733): a
    ``crates/*/tests`` directory that is never scanned never fails, so this
    makes coverage of the tests trees a checked invariant too (#16745).
    """
    checks = checks if checks is not None else TESTS_LINE_LIMIT_CHECKS
    registered = {base.resolve() for _name, base, _limit, *_rest in checks}
    missing = []
    for tests_dir in crate_tests_roots(crates_dir):
        if tests_dir.resolve() not in registered:
            missing.append(tests_dir.relative_to(ROOT).as_posix())
    return missing


# `scripts/` coverage (#16745). A single root rather than one-per-crate, and
# scripts are not Rust, so this needs its own extension-aware walk instead of
# `iter_rs_files`/`scan_line_limits`.
SCRIPTS_LINE_LIMIT = SRC_LINE_LIMIT
SCRIPT_FILE_EXTENSIONS = (".py", ".sh", ".mjs", ".cjs", ".js", ".ts", ".ps1")

SCRIPTS_LINE_LIMIT_CHECKS = [
    (
        "Scripts boundary: scripts/ files must stay under 2000 LOC (#16745)",
        ROOT / "scripts",
        SCRIPTS_LINE_LIMIT,
        set(),
    ),
]


def iter_script_files(base: pathlib.Path, extensions=SCRIPT_FILE_EXTENSIONS):
    for path in base.rglob("*"):
        if not path.is_file() or path.suffix not in extensions:
            continue
        rel = path.relative_to(ROOT).as_posix()
        parts = set(rel.split("/"))
        if EXCLUDE_DIRS.intersection(parts):
            continue
        yield path, rel


def scan_script_line_limits(
    base: pathlib.Path,
    limit: int,
    exclude_files=None,
    extensions=SCRIPT_FILE_EXTENSIONS,
):
    hits = []
    for path, rel in iter_script_files(base, extensions):
        if exclude_files and rel in exclude_files:
            continue
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


# Pin field counts on giant coordination structs so workstream-4 (Checker
# State / Speculation) extraction work shows up as visible metric drift in
# the diff.  Each entry: (description, file_path, struct_name, max_fields).
#
# When a field is added intentionally, bump the cap in the same PR.  This is
# the same convention as `FILE_LINE_LIMIT_CHECKS` — it makes architecture
# health metric drift visible at review time (Operating Principle 8 in
# `docs/plan/ROADMAP.md`).
STRUCT_FIELD_COUNT_CHECKS = [
    (
        "Checker boundary: CheckerContext field count (architecture health metric 1)",
        ROOT / "crates" / "tsz-checker" / "src" / "context" / "mod.rs",
        "CheckerContext",
        # 255 -> 256: `parameter_grammar_suppress_spans` records the rest
        # parameters whose parser-emitted grammar diagnostics tsc's single
        # early-return `checkGrammarParameterList` never reached, so the driver
        # can drop them (#16644).
        # 256 -> 257: `suppress_unreachable_reporting` mirrors tsc's
        # `withinUnreachableCode` — TS7027 stays silent beneath a statement
        # already covered by a reported unreachable range (namespace bodies,
        # class member bodies) while normal checking continues.
        257,
    ),
]

# Pin the size of the solver's full database capability trait while #8205
# splits it into narrower storage/config/provenance traits.  The live count is
# tolerated as baseline debt, but new methods must either land on a narrower
# trait or deliberately bump this cap with a roadmap/issue explanation.
#
# Each entry: (description, file_path, trait_name, max_methods).
TRAIT_METHOD_COUNT_CHECKS = [
    (
        "Solver boundary: TypeDatabase method count (#8205)",
        ROOT / "crates" / "tsz-solver" / "src" / "caches" / "db.rs",
        "TypeDatabase",
        # Ratcheted 80 -> 75 by the #15643 arch-health paydown: lib-builtin
        # registry accessors split onto the narrower `TypeBuiltinAccess`
        # supertrait.
        75,
    ),
]

VALID_CHECKER_CONTEXT_LIFETIMES = {
    "ProgramStable",
    "WorkerReusable",
    "FileLocalReset",
    "SpeculationScoped",
    "DiagnosticsOnly",
    "LspPersistent",
}

VALID_CHECKER_CONTEXT_CAPABILITIES = {
    "CheckerInputs",
    "DiagnosticState",
    "EmitSummaryState",
    "FileTypeCache",
    "FlowSessionState",
    "ProgramLookupContext",
    "RelationSessionState",
    "SpeculationState",
}

CHECKER_CONTEXT_LIFETIME_MANIFEST_CHECKS = [
    (
        "Checker boundary: CheckerContext lifetime inventory (T2.1.A)",
        ROOT / "crates" / "tsz-checker" / "src" / "context" / "mod.rs",
        "CheckerContext",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "context"
        / "checker_context_lifetimes.toml",
    ),
]

# Pin the count of files that construct full independent parse→bind→check
# pipelines (architecture health metric 4 in `docs/plan/ROADMAP.md`).  A
# "full pipeline" is any non-test source file that calls all three of
# `ParserState::new`, `BinderState::new`, `CheckerState::new` — that is, a
# frontend reaching past the compiler service into the raw crate APIs.
#
# Workstream 3 ("Compiler Service Front Door") exit criterion is "There is
# one blessed parse-bind-check path."  Pinning the count makes new
# independent pipelines fail pre-commit and consolidation work show up as
# a cap reduction in the same diff.
#
# Each entry: (description, search_roots, max_pipelines).
INDEPENDENT_PIPELINE_CHECKS = [
    (
        "Frontend boundary: independent parse-bind-check pipelines (architecture health metric 4)",
        [
            ROOT / "crates" / "tsz-cli" / "src",
            ROOT / "crates" / "tsz-core" / "src",
            ROOT / "crates" / "tsz-lsp" / "src",
            ROOT / "crates" / "tsz-wasm" / "src",
        ],
        4,
    ),
]

# Pin the count of non-test source files that import `tsz_solver` outside the
# solver/checker boundary (architecture health metric 7 in
# `docs/plan/ROADMAP.md`).  The checker crate contains the canonical
# `query_boundaries` modules and is the one architecturally allowed consumer
# of solver internals; every other crate (`tsz-cli`, `tsz-core`, `tsz-lsp`,
# `tsz-wasm`, `tsz-emitter`, `tsz-lowering`) reaching directly into the solver
# weakens the front door story (workstream 3) and shows up as drift on this
# metric.
#
# A file "imports tsz_solver" if a non-comment line contains one of:
#   - `use tsz_solver::...`
#   - `pub use tsz_solver` (re-export, including `pub use tsz_solver;`)
#   - `extern crate tsz_solver`
#
# Each entry: (description, search_roots, exclude_path_prefixes, max_imports).
SOLVER_IMPORT_COUNT_CHECKS = [
    (
        "Frontend/emitter boundary: direct tsz_solver imports outside solver/checker (architecture health metric 7)",
        [ROOT / "crates"],
        (
            "crates/tsz-solver/",
            "crates/tsz-checker/",
        ),
        35,
    ),
]

# Pin the count of flat root-level solver computation API references outside
# the approved checker query-boundary layer. Existing references are
# transitional compatibility debt from `tsz_solver::*` root re-exports; new
# references should go through a named solver facade, a checker
# `query_boundaries` helper, or intentionally bump this cap.
#
# Each entry:
#   (description, search_roots, exclude_path_prefixes, max_references).
ROOT_SOLVER_COMPUTATION_IMPORT_COUNT_CHECKS = [
    (
        "Solver API boundary: flat root computation imports outside query boundaries (#8204)",
        [
            ROOT / "crates" / "tsz-checker" / "src",
            ROOT / "crates" / "tsz-emitter" / "src",
            ROOT / "crates" / "tsz-lsp" / "src",
            ROOT / "crates" / "tsz-cli" / "src",
        ],
        ("crates/tsz-checker/src/query_boundaries/",),
        0,
    ),
]

# Seal the module-path escape hatch around the #8204 tiered solver API: the
# flat-root ratchet above pins re-exported symbol names, but the solver still
# declares `pub mod operations`/`relations`/`evaluation`/..., so downstream
# crates can reach computation internals by full module path
# (`tsz_solver::operations::widening::widen_type`). This pins those references
# in the emitter/LSP/CLI/wasm source trees at the current count; migrating a
# site to a tiered facade or checker query-boundary helper should ratchet the
# cap down in the same diff.
#
# Transitional exceptions remain at 14 module-path references. Ratchet this
# down as sites migrate to named solver facades or checker query-boundaries.
#
# Each entry:
#   (description, search_roots, exclude_path_prefixes, max_references).
MODULE_PATH_SOLVER_COMPUTATION_IMPORT_COUNT_CHECKS = [
    (
        "Solver API boundary: module-path computation imports in emitter/LSP/CLI/wasm (#8204)",
        [
            ROOT / "crates" / "tsz-emitter" / "src",
            ROOT / "crates" / "tsz-lsp" / "src",
            ROOT / "crates" / "tsz-cli" / "src",
            ROOT / "crates" / "tsz-wasm" / "src",
        ],
        (),
        14,
    ),
]

# Pin the producer-side compatibility surface that still re-exports solver
# computation/construction APIs from the crate root. The zero wildcard guard
# below prevents broad `pub use module::*` growth; this count makes explicit
# root re-export growth visible too.
#
# Each entry:
#   (description, file_path, root_module_prefixes, max_reexports).
ROOT_SOLVER_EXPLICIT_REEXPORT_COUNT_CHECKS = [
    (
        "Solver API boundary: flat root explicit computation re-exports (#8204)",
        ROOT / "crates" / "tsz-solver" / "src" / "lib.rs",
        (
            "caches",
            "canonicalize",
            "classes",
            "contextual",
            "evaluation",
            "instantiation",
            "intern",
            "narrowing",
            "objects",
            "operations",
            "relations",
            "widening",
        ),
        0,
    ),
]

# Pin direct checker call sites into `query_boundaries::common`, the broad
# compatibility/quarantine barrel tracked by #8225. Existing sites are
# tolerated as migration debt; new checker code should prefer a narrower
# request-shaped boundary module, or intentionally bump this cap.
# Ratcheted 3082 -> 3050 by the #15643 arch-health paydown: FunctionShape
# instantiation / parameter-list / redeclaration-widening callers now route
# through their domain boundaries instead of the common barrel, and the
# arch-smoke run caught remaining live-count slack.
# Ratcheted 3050 -> 3049 by the 2026-07-14 goal campaign: the declaration-emit
# portability walk, nested-application display, and object-literal request-facts
# callers now route through the `diagnostics` boundary.
QUERY_BOUNDARY_COMMON_REFERENCE_BASELINE = 3049

# Temporary green-campaign headroom for #14351. Guard tests keep the baseline
# tight underneath this reserve, so reductions still force ratchets while urgent
# parity fixes have a small explicit budget instead of repeatedly deadlocking.
QUERY_BOUNDARY_COMMON_REFERENCE_GREEN_HEADROOM = 16

#
# Each entry:
#   (description, search_roots, exclude_path_prefixes, max_references).
QUERY_BOUNDARY_COMMON_REFERENCE_COUNT_CHECKS = [
    (
        "Checker query boundary: direct common quarantine references outside query_boundaries (#8225)",
        [ROOT / "crates" / "tsz-checker" / "src"],
        ("crates/tsz-checker/src/query_boundaries/",),
        # Historical bump/ratchet details live in the PRs linked from #8225 and
        # #12948. The current live baseline is 3082 (through the enum-member
        # quick-info display fix); #8225 narrowing remains the removal condition.
        #
        # #14351 temporary green-campaign budget: keep the 3082 live baseline
        # pinned, but allow a small explicit reserve for parity fixes that add
        # sanctioned checker→solver queries before #8225 drain slices make room.
        QUERY_BOUNDARY_COMMON_REFERENCE_BASELINE
        + QUERY_BOUNDARY_COMMON_REFERENCE_GREEN_HEADROOM,
    ),
]

# Pin root-level lint allowance entries in the query-boundary module map. #8225
# tracks turning this layer from migration quarantine into narrower APIs, and
# broad module-level allowances are part of that quarantine debt. The cap should
# ratchet down as modules no longer need blanket suppressions.
QUERY_BOUNDARY_MODULE_ALLOWANCE_COUNT_CHECKS = [
    (
        "Checker query boundary: module-level lint allowances must not grow (#8225)",
        ROOT / "crates" / "tsz-checker" / "src" / "query_boundaries" / "mod.rs",
        0,
    ),
]

WORKSPACE_CLIPPY_ALLOW_COUNT_CHECKS = [
    (
        "Workspace Clippy suppressions must not grow (#9446)",
        [ROOT / "crates"],
        # Bumped 10 -> 11 for the JSX special-attribute display split: the new
        # `check_assignable_or_report_at_with_display_types_and_options` helper
        # legitimately takes eight explicit parameters (real + display source /
        # target, two node indices, two display-policy flags), which with the
        # receiver exceeds the workspace `too-many-arguments-threshold = 8`, so
        # a single `#[allow(clippy::too_many_arguments)]` is required.
        11,
    ),
]

# Architecture health metric 5 (workstream 4): non-test checker files that call a
# low-level speculation-rollback API directly instead of through a capability-
# scoped holder (`FullSpeculationSnapshot` / `DiagnosticSpeculationSnapshot`). The
# cap ratchets down toward 0 as call sites migrate; the scanner names the
# offending files when it fires. Lowered 4 -> 3 (live count 5 -> 3) after
# `elaboration.rs` and `overload_resolution/helpers.rs` moved their full-snapshot
# rollbacks onto `FullSpeculationSnapshot`. The remaining callers additionally use
# diag-merge APIs (`rollback_and_replace_diagnostics` / `restore_ts2454_state`)
# the holder surface does not yet expose, so unlisting them needs a richer holder
# rather than a mechanical swap.
SNAPSHOT_ROLLBACK_FILE_COUNT_CHECKS = [
    (
        "Checker speculation boundary: snapshot-rollback call sites outside speculation.rs (architecture health metric 5)",
        [ROOT / "crates" / "tsz-checker" / "src"],
        ("crates/tsz-checker/src/context/speculation.rs",),
        3,
    ),
]

# Pin architecture health metric 6 ("Speculation APIs with surprising
# non-RAII behavior") in `docs/plan/ROADMAP.md`.
#
# After PR #1213 renamed `DiagnosticSpeculationGuard → DiagnosticSpeculationSnapshot`
# the speculation surface no longer carries `…Guard` types whose name
# implies RAII rollback-on-drop while the implementation is implicit-commit.
# This guard pins the rename: any new `pub(crate) struct …Guard` on the
# speculation surface re-introduces the same ambiguity and must update
# the cap (deliberately) or use a `…Snapshot` name (preferred). The
# scan looks at the speculation file directly so the check is local
# and cheap.
#
# Each entry: (description, file_path, max_guard_struct_count).
SPECULATION_GUARD_NAME_CHECKS = [
    (
        "Checker speculation boundary: number of `…Guard` structs in speculation.rs (architecture health metric 6)",
        ROOT / "crates" / "tsz-checker" / "src" / "context" / "speculation.rs",
        0,
    ),
]

DEBUG_PRINT_REPORT_PATH = ROOT / "scripts" / "perf" / "debug-print-report.py"
DEBUG_PRINT_MACRO_CHECKS = [
    (
        "Performance boundary: compiler-internal debug print macros (Track 10)",
        ROOT,
        (
            "crates/tsz-binder/src",
            "crates/tsz-checker/src",
            "crates/tsz-common/src",
            "crates/tsz-core/src",
            "crates/tsz-emitter/src",
            "crates/tsz-lowering/src",
            "crates/tsz-parser/src",
            "crates/tsz-scanner/src",
            "crates/tsz-solver/src",
        ),
    ),
]

CFG_TEST_GATED_PATH_MOD_CHECKS = [
    (
        "Test-module gate: every `#[path = \"…tests/…\"] mod x;` declaration "
        "must be immediately preceded by #[cfg(test)] (#16121 — a same-point "
        "merge conflict can silently drop the gate and compile a test-only "
        "module into non-test builds)",
        tuple(sorted((ROOT / "crates").glob("*/src/lib.rs"))),
    ),
]

# Pin Track 10's diagnostic-debt ratchets in the shared architecture guard.
# These are count metrics, not new semantic bans: the current baselines still
# contain legacy fingerprint rewrites, source-text snippets, and rendered-type
# decisions. Any new line must bump the cap intentionally; cleanup PRs should
# lower the cap in the same diff.
#
# Each entry: (description, search_roots, pattern, max_lines).
REGEX_LINE_COUNT_CHECKS = [
    (
        "Checker diagnostic boundary: post-check rewrite_*_fingerprints functions (Track 10)",
        [ROOT / "crates" / "tsz-checker" / "src"],
        re.compile(r"^\s*fn\s+rewrite_\w+_fingerprints\s*\("),
        5,
    ),
    (
        "Checker diagnostic boundary: source_text.contains decisions (Track 10)",
        [ROOT / "crates" / "tsz-checker" / "src"],
        re.compile(r"\bsource_text\.contains\s*\("),
        20,
    ),
    (
        "Checker diagnostic boundary: file-name/path substring decisions (Track 10)",
        [ROOT / "crates" / "tsz-checker" / "src"],
        re.compile(r"\b(?:\w+\.)?file_name\.contains\s*\(|\bsource_path\.contains\s*\("),
        1,
    ),
    (
        "Checker diagnostic boundary: rendered type strings as semantic input (Track 10)",
        [ROOT / "crates" / "tsz-checker" / "src"],
        re.compile(
            r"\bformat_type(?:_diagnostic)?\s*\([^\n]*"
            r"(?:\.contains\s*\(|\.starts_with\s*\(|\.ends_with\s*\(|\.as_str\s*\(\))"
        ),
        0,
    ),
    (
        "Checker diagnostic boundary: rendered message predicates (Track 10)",
        [
            ROOT / "crates" / "tsz-checker" / "src" / "checkers" / "jsx",
            ROOT / "crates" / "tsz-checker" / "src" / "checkers" / "call_checker",
            ROOT / "crates" / "tsz-checker" / "src" / "types" / "type_checking",
        ],
        re.compile(
            r"\b(?:display|source_display|target_display|stripped_display|"
            r"diagnostic\.message_text|raw|evaluated)"
            r"\.(?:contains|starts_with|ends_with|as_str)\s*\("
        ),
        14,
    ),
    (
        # Broadens the previous direct-call guard to include sliced source-text
        # recovery predicates such as `source_text[start..end].contains(...)`.
        # The current baseline is pinned so each migration to structured emit
        # facts can ratchet this count down in the same PR.
        "Emitter boundary: source_text contains recovery decisions (Track 9/10)",
        [ROOT / "crates" / "tsz-emitter" / "src"],
        re.compile(r"\bsource_text(?:\[[^\n\]]+\])?\.contains\s*\("),
        0,
    ),
    (
        # `set_export_surface` is the one documented compatibility bridge for
        # older declaration-emitter callers. New DTS facts should enter through
        # `DeclarationSummary` query methods instead of growing raw
        # `ExportSurface` handoff points.
        "Emitter DTS boundary: raw ExportSurface handoff compatibility bridge (#8275)",
        [ROOT / "crates" / "tsz-emitter" / "src" / "declaration_emitter"],
        re.compile(r"\bfn\s+set_export_surface\s*\("),
        1,
    ),
    (
        "Emitter boundary: recovered variable typeof tails use parser facts (#8276)",
        [
            ROOT
            / "crates"
            / "tsz-emitter"
            / "src"
            / "emitter"
            / "statements"
            / "recovered_variable_statement.rs"
        ],
        re.compile(
            r"\b(?:find_source_pattern_outside_quoted_text|find_matching_source_paren|skip_quoted_source_text)\b"
        ),
        0,
    ),
    (
        # The async ES5 lowering path still has one source-text fallback for
        # recovered `yield` detection. Keep it visible so the future parser- or
        # lowering-owned fact can ratchet this to zero in the same PR.
        "Emitter boundary: async yield source-text fallback (#8276)",
        [
            ROOT
            / "crates"
            / "tsz-emitter"
            / "src"
            / "transforms"
            / "async_es5_ir_discovery.rs"
        ],
        re.compile(r"\bfn\s+node_text_contains_yield\s*\("),
        1,
    ),
    (
        "Solver API boundary: flat root wildcard compatibility re-exports (#8204)",
        [ROOT / "crates" / "tsz-solver" / "src" / "lib.rs"],
        re.compile(r"^pub use (?:[A-Za-z_][A-Za-z0-9_]*::)+\*;"),
        0,
    ),
    (
        "Solver API boundary: root judge convenience re-export (#8204)",
        [ROOT / "crates" / "tsz-solver" / "src" / "lib.rs"],
        re.compile(r"^\s*pub\s+mod\s+judge\s*\{"),
        0,
    ),
    (
        "Solver relation boundary: legacy relation flag bridge surface (#8207)",
        [ROOT / "crates" / "tsz-solver" / "src"],
        re.compile(
            r"\b(?:from_checker_flags_u16|from_legacy_u8|to_legacy_u8|"
            r"subtype_cache_config_from_legacy_flags|"
            r"assignability_cache_config_from_legacy_flags)\b"
        ),
        0,
    ),
    (
        "Solver relation boundary: RelationPolicy must store typed flags (#8207)",
        [ROOT / "crates" / "tsz-solver" / "src" / "relations" / "relation_queries.rs"],
        re.compile(r"^\s*(?:pub\s+)?flags\s*:\s*u16\s*,"),
        0,
    ),
    (
        "Solver relation boundary: RelationPolicy must not expose packed flags (#8207)",
        [ROOT / "crates" / "tsz-solver" / "src" / "relations" / "relation_queries.rs"],
        re.compile(r"\bfn\s+legacy_packed_flags\s*\([^)]*\)\s*->\s*u16\b"),
        0,
    ),
    (
        "Checker relation boundary: raw diagnostic assignability predicates (#8227)",
        [
            ROOT
            / "crates"
            / "tsz-checker"
            / "src"
            / "assignability"
            / "assignability_diagnostics.rs",
            ROOT / "crates" / "tsz-checker" / "src" / "error_reporter",
            ROOT / "crates" / "tsz-checker" / "src" / "checkers" / "jsx",
        ],
        re.compile(
            r"\b(?:self|self\.ctx\.types|self\.interner)"
            r"\.is_assignable_to(?:_[A-Za-z0-9_]+)?\s*\("
        ),
        0,
    ),
    (
        "Checker residency boundary: with_parent_cache_attributed migration callsites (Track 10)",
        [ROOT / "crates" / "tsz-checker" / "src"],
        re.compile(
            r"^(?!\s*(?:pub(?:\([^)]*\))?\s+)?fn\b)"
            r".*\bwith_parent_cache_attributed\s*\("
        ),
        # Ratcheted 28 -> 16 by the #15643 arch-health paydown: fifteen
        # delegation sites consolidated onto the single
        # `CheckerState::delegate_for_arena` factory.
        16,
    ),
    (
        "Checker residency boundary: copy_symbol_file_targets_to_attributed migration callsites (Track 10)",
        [ROOT / "crates" / "tsz-checker" / "src"],
        re.compile(
            r"^(?!\s*(?:pub(?:\([^)]*\))?\s+)?fn\b)"
            r".*\bcopy_symbol_file_targets_to_attributed\s*\("
        ),
        # Ratcheted 18 -> 5 by the #15643 arch-health paydown (see
        # `CheckerState::delegate_for_arena`).
        5,
    ),
    (
        "Checker relation boundary: diagnostic-local RelationRequest constructors (#8227)",
        [
            ROOT
            / "crates"
            / "tsz-checker"
            / "src"
            / "assignability"
            / "assignability_diagnostics.rs",
            ROOT / "crates" / "tsz-checker" / "src" / "error_reporter",
            ROOT / "crates" / "tsz-checker" / "src" / "checkers" / "jsx",
        ],
        re.compile(r"\bRelationRequest::[A-Za-z_][A-Za-z0-9_]*\s*\("),
        0,
    ),
    (
        "Solver relation boundary: legacy packed relation flag bridges (#8207)",
        [ROOT / "crates" / "tsz-solver" / "src"],
        re.compile(
            r'^(?:[^"\n]|"[^"\n]*")*?'
            r"\b(?:subtype_cache_config_from_legacy_flags\s*\(|"
            r"assignability_cache_config_from_legacy_flags\s*\(|"
            r"from_checker_flags_u16\s*\(|from_legacy_u8\s*\(|to_legacy_u8\s*\(|"
            r"RelationCacheKey::(?:subtype|assignability)\s*\(|"
            r"RelationFlags::from_bits_truncate\s*\(|"
            r"CachedAnyMode::from_legacy_u8\s*\()"
        ),
        0,
    ),
    (
        "Solver relation boundary: relation engines avoid packed apply_flags (#8207)",
        [ROOT / "crates" / "tsz-solver" / "src" / "relations"],
        re.compile(
            r"\bfn\s+apply_flags\s*\([^)]*\bflags\s*:\s*u16"
            r"|\.\s*apply_flags\s*\(\s*policy\.flags\s*\)"
        ),
        0,
    ),
    (
        "Solver relation boundary: legacy flag decoder avoids cache-key constants (#8207)",
        [ROOT / "crates" / "tsz-solver" / "src" / "relations" / "relation_queries.rs"],
        re.compile(r"\bRelationCacheKey::FLAG_[A-Z0-9_]+\b"),
        0,
    ),
    (
        "Solver relation boundary: legacy RelationPolicy::from_flags calls stay at boundary (#8207)",
        [ROOT / "crates" / "tsz-solver" / "src"],
        re.compile(
            r'^\s*(?!//)(?:[^"\n]|"[^"\n]*")*?'
            r"\bRelationPolicy::from_flags\s*\("
        ),
        0,
    ),
    (
        "Solver relation boundary: query cache uses relation facade (#8207)",
        [ROOT / "crates" / "tsz-solver" / "src" / "caches" / "query_cache.rs"],
        re.compile(r"\b(?:configured_compat_checker|configured_subtype_checker)\s*\("),
        0,
    ),
    (
        "Solver relation boundary: query cache trace labels use typed policy names (#8207)",
        [ROOT / "crates" / "tsz-solver" / "src" / "caches" / "query_cache.rs"],
        re.compile(r'"is_(?:subtype_of|assignable_to)_with_flags"'),
        0,
    ),
    (
        "Solver relation boundary: query cache legacy flag overrides (#8207)",
        [ROOT / "crates" / "tsz-solver" / "src" / "caches" / "query_cache.rs"],
        re.compile(r"\bfn\s+is_(?:subtype_of|assignable_to)_with_flags\s*\("),
        0,
    ),
    (
        "Solver relation boundary: query database legacy flag methods (#8207)",
        [
            ROOT / "crates" / "tsz-solver" / "src" / "caches" / "db.rs",
            ROOT / "crates" / "tsz-solver" / "src" / "caches" / "query_cache.rs",
        ],
        re.compile(r"\bfn\s+is_(?:subtype_of|assignable_to)_with_flags\s*\("),
        0,
    ),
]

# Track 10 performance guardrail: branch-local `visited.clone()` traversal
# clones are a known scale-cliff risk for graph predicates.  Existing sites are
# pinned by file plus statement text so normal line movement does not churn the
# guard, while new clone sites must either replace an existing one with a
# memoized/worklist traversal or extend this allowlist intentionally.
BRANCH_LOCAL_VISITED_CLONE_CHECKS = [
    (
        "Performance boundary: branch-local visited.clone() graph traversal sites (Track 10)",
        [
            ROOT / "crates" / "tsz-checker" / "src",
            ROOT / "crates" / "tsz-lsp" / "src",
            ROOT / "crates" / "tsz-solver" / "src",
        ],
        (
            (
                "crates/tsz-checker/src/state/type_environment/lazy.rs",
                "let mut branch_visited = visited.clone();",
            ),
            (
                "crates/tsz-checker/src/state/type_resolution/module.rs",
                "let mut inner_visited = visited.clone();",
            ),
            (
                "crates/tsz-checker/src/types/queries/type_only.rs",
                "let mut exists_visited = visited.clone();",
            ),
            (
                "crates/tsz-checker/src/types/queries/type_only.rs",
                "let mut type_only_visited = visited.clone();",
            ),
            (
                "crates/tsz-solver/src/evaluation/evaluate_rules/infer_pattern.rs",
                "let mut alias_visited = visited.clone();",
            ),
            (
                "crates/tsz-solver/src/evaluation/evaluate_rules/infer_pattern_object_helpers.rs",
                "let mut alias_visited = visited.clone();",
            ),
        ),
    ),
]

# Pin the count of LSP feature-dispatch methods in
# `crates/tsz-lsp/src/project/features.rs` (architecture health metric 7
# in `docs/plan/ROADMAP.md` — "LSP/WASM semantic features implemented
# outside the compiler service layer").
#
# Every `pub fn` on `Project` whose name starts with one of `get_`,
# `provide_`, `prepare_`, `handle_`, `on_`, `find_`, or `resolve_` is an
# LSP feature dispatched directly from `Project` rather than through a
# service-trait abstraction. Workstream 6 ("LSP And WASM As Service
# Clients") exit criterion 3 is that "LSP request handling mostly maps
# protocol inputs to service queries and service outputs to protocol
# DTOs"; the raw count tracks how far the live code is from that
# state. Each new feature dispatch must bump the cap with a roadmap
# entry; consolidation onto a service trait shows up as a cap reduction
# in the same diff.
#
# Each entry: (description, file_path, max_methods).
LSP_FEATURE_METHOD_COUNT_CHECKS = [
    (
        "LSP boundary: feature-dispatch method count in project/features.rs (architecture health metric 7)",
        ROOT / "crates" / "tsz-lsp" / "src" / "project" / "features.rs",
        32,
    ),
]

PROJECT_DASHBOARD_ROW_CHECKS = [
    (
        "Project corpus dashboard: shared project row manifest must cover dashboard rows (Track 1)",
        ROOT / "scripts" / "bench" / "project-rows.mjs",
    ),
]

PROJECT_FIXTURE_SOURCE_CHECKS = [
    (
        "Project corpus fixtures: pinned rows must record fixture source refs (Track 1)",
        ROOT / "scripts" / "bench" / "project-rows.mjs",
        ROOT / "scripts" / "bench" / "project-fixtures.sh",
    ),
]

PROJECT_INCLUSION_POLICY_CHECKS = [
    (
        "Project corpus inclusion: row manifest must match compile guard and benchmark rows (Track 1)",
        ROOT / "scripts" / "bench" / "project-rows.mjs",
        ROOT / "scripts" / "ci" / "project-compile-guard.sh",
        ROOT / "scripts" / "bench" / "bench-vs-tsgo.sh",
    ),
]

PROJECT_CONFIG_WRITER_CHECKS = [
    (
        "Project corpus config shape: shared rows must use shared config writers (Track 1)",
        ROOT / "scripts" / "bench" / "project-fixtures.sh",
        ROOT / "scripts" / "ci" / "project-compile-guard.sh",
        ROOT / "scripts" / "bench" / "bench-vs-tsgo.sh",
    ),
]

PROJECT_CONFIG_WRITERS = {
    "utility-types-project": "tsz_write_utility_types_config",
    "ts-toolbelt-project": "tsz_write_ts_toolbelt_config",
    "ts-essentials-project": "tsz_write_ts_essentials_config",
    "rxjs-project": "tsz_write_rxjs_config",
    "type-fest-project": "tsz_write_type_fest_config",
    "zod-project": "tsz_write_zod_config",
    "kysely-project": "tsz_write_kysely_config",
    "nextjs": "tsz_write_nextjs_config",
}

GENERATED_PROJECT_ROWS_WITHOUT_PINNED_SOURCE = {
    "vite-vanilla-ts-app",
    "nextjs-fresh-app",
}

COMPILE_GUARD_ONLY_PROJECT_ROWS = {
    "type-challenges-solutions-project",
}

BENCHMARK_ONLY_PROJECT_ROWS = {
    "nextjs",
    "large-ts-repo",
}

EXCLUDE_DIRS = {".git", "target", "node_modules"}
SOLVER_TYPEDATA_QUARANTINE_ALLOWLIST = {
    "crates/tsz-solver/src/intern/mod.rs",
    "crates/tsz-solver/src/intern/core/constructors.rs",
    "crates/tsz-solver/src/intern/intersection.rs",
    "crates/tsz-solver/src/intern/normalize.rs",
    "crates/tsz-solver/src/intern/template.rs",
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

    if excludes.get("exclude_test_files") and is_test_file(rel):
        return matches

    for i, line in enumerate(file_text.splitlines(), start=1):
        if excludes.get("ignore_comment_lines", False):
            if line.lstrip().startswith("//"):
                continue
        if pattern.search(line):
            matches.append(i)
    return matches


def is_test_file(rel: str) -> bool:
    """Check if a file path looks like a test file."""
    parts = rel.split("/")
    filename = parts[-1] if parts else ""
    return filename.endswith("_tests.rs") or filename.startswith("test_")


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


def scan_line_limits(base: pathlib.Path, limit: int, exclude_files=None):
    hits = []
    for path, rel in iter_rs_files(base):
        if exclude_files and rel in exclude_files:
            continue
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
