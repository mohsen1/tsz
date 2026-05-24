#!/usr/bin/env python3
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

LINE_LIMIT_CHECKS = [
    (
        "Checker boundary: src files must stay under 2000 LOC",
        ROOT / "crates" / "tsz-checker" / "src",
        2000,
        # Exclusion list pruned 2026-05-01: removed 15 entries for files
        # that no longer exist (split or renamed) and 16 entries for files
        # that have since dropped below 2000 lines.
        #
        # Refreshed 2026-05-12: removed entries that had since dropped below
        # 2000 raw lines. The set below is the audited current set of files
        # above 2000 raw lines.
        {
            # ≥2000 LOC, real files. When a file drops below the limit,
            # delete it from this set in the same diff and the
            # `test_excluded_files_actually_exceed_limit` test will catch
            # any regression.
            "crates/tsz-checker/src/assignability/assignability_checker.rs",
            "crates/tsz-checker/src/assignability/assignability_diagnostics.rs",
            "crates/tsz-checker/src/checkers/jsx/tests.rs",
            "crates/tsz-checker/src/checkers/jsx/props/resolution.rs",
            "crates/tsz-checker/src/classes/class_checker.rs",
            "crates/tsz-checker/src/declarations/import/declaration.rs",
            "crates/tsz-checker/src/error_reporter/call_errors/display_formatting.rs",
            "crates/tsz-checker/src/error_reporter/call_errors/elaboration.rs",
            "crates/tsz-checker/src/error_reporter/properties.rs",
            "crates/tsz-checker/src/error_reporter/render_failure.rs",
            "crates/tsz-checker/src/flow/control_flow/core.rs",
            "crates/tsz-checker/src/jsdoc/diagnostics.rs",
            "crates/tsz-checker/src/jsdoc/params.rs",
            "crates/tsz-checker/src/state/state_checking/class.rs",
            "crates/tsz-checker/src/state/state_checking/property.rs",
            "crates/tsz-checker/src/state/state_checking_members/interface_checks.rs",
            "crates/tsz-checker/src/state/type_analysis/computed_helpers.rs",
            "crates/tsz-checker/src/state/type_analysis/core.rs",
            "crates/tsz-checker/src/state/type_environment/core.rs",
            "crates/tsz-checker/src/state/type_resolution/module.rs",
            "crates/tsz-checker/src/state/variable_checking/core.rs",
            "crates/tsz-checker/src/state/variable_checking/destructuring.rs",
            "crates/tsz-checker/src/tests/architecture_contract_tests.rs",
            "crates/tsz-checker/src/tests/dispatch_tests.rs",
            "crates/tsz-checker/src/types/class_type/constructor.rs",
            "crates/tsz-checker/src/types/class_type/core.rs",
            "crates/tsz-checker/src/types/computation/binary.rs",
            "crates/tsz-checker/src/types/computation/call/inner.rs",
            "crates/tsz-checker/src/types/computation/call_inference.rs",
            "crates/tsz-checker/src/types/computation/object_literal/computation.rs",
            "crates/tsz-checker/src/types/function_type.rs",
            "crates/tsz-checker/src/types/property_access_type/resolve.rs",
            "crates/tsz-checker/src/types/queries/core.rs",
            "crates/tsz-checker/src/types/queries/lib.rs",
            "crates/tsz-checker/src/types/type_checking/duplicate_identifiers.rs",
            "crates/tsz-checker/src/types/type_checking/duplicate_identifiers_helpers.rs",
            "crates/tsz-checker/src/types/utilities/core.rs",
            "crates/tsz-checker/src/types/utilities/enum_utils.rs",
        },
    ),
    (
        "Checker computation boundary: type-computation monoliths must stay below 3100 LOC (#8226)",
        ROOT / "crates" / "tsz-checker" / "src" / "types" / "computation",
        3100,
    ),
]

FILE_LINE_LIMIT_CHECKS = [
    (
        "Core boundary: tsz-core lib facade must stay at current 365 LOC baseline",
        ROOT / "crates" / "tsz-core" / "src" / "lib.rs",
        365,
    ),
    (
        "Checker query boundary: common quarantine must not grow (#8225)",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "query_boundaries"
        / "common.rs",
        1920,
    ),
    (
        "Solver engine boundary: generic call resolver must stay at current 3381 LOC baseline (#8209)",
        ROOT
        / "crates"
        / "tsz-solver"
        / "src"
        / "operations"
        / "generic_call"
        / "resolve.rs",
        3381,
    ),
    # Pin the async ES5 IR transformer file size while #8277 splits the
    # monolith into staged lowering modules. The cap should ratchet down
    # as more phases (helper scheduling, temp/hoist planning, suspended
    # target lowering, ...) are extracted into sibling submodules.
    (
        "Emitter boundary: async ES5 IR engine size ratchet (#8277)",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "transforms"
        / "async_es5_ir.rs",
        5150,
    ),
]

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
        237,
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
        77,
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
        36,
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
#
# Each entry:
#   (description, search_roots, exclude_path_prefixes, max_references).
QUERY_BOUNDARY_COMMON_REFERENCE_COUNT_CHECKS = [
    (
        "Checker query boundary: direct common quarantine references outside query_boundaries (#8225)",
        [ROOT / "crates" / "tsz-checker" / "src"],
        ("crates/tsz-checker/src/query_boundaries/",),
        # Current post-merge count on the logical-assignment typeof query PR.
        # Keep this ratchet exact when the broad boundary debt shrinks.
        3378,
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
        107,
    ),
]

SNAPSHOT_ROLLBACK_FILE_COUNT_CHECKS = [
    (
        "Checker speculation boundary: snapshot-rollback call sites outside speculation.rs (architecture health metric 5)",
        [ROOT / "crates" / "tsz-checker" / "src"],
        ("crates/tsz-checker/src/context/speculation.rs",),
        6,
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
        9,
    ),
    (
        "Checker diagnostic boundary: source_text.contains decisions (Track 10)",
        [ROOT / "crates" / "tsz-checker" / "src"],
        re.compile(r"\bsource_text\.contains\s*\("),
        36,
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
        "Emitter boundary: source_text.contains recovery decisions (Track 9/10)",
        [ROOT / "crates" / "tsz-emitter" / "src"],
        re.compile(r"\bsource_text\.contains\s*\("),
        3,
    ),
    (
        "Solver API boundary: flat root wildcard compatibility re-exports (#8204)",
        [ROOT / "crates" / "tsz-solver" / "src" / "lib.rs"],
        re.compile(r"^pub use (?:[A-Za-z_][A-Za-z0-9_]*::)+\*;"),
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
        33,
    ),
    (
        "Checker residency boundary: copy_symbol_file_targets_to_attributed migration callsites (Track 10)",
        [ROOT / "crates" / "tsz-checker" / "src"],
        re.compile(
            r"^(?!\s*(?:pub(?:\([^)]*\))?\s+)?fn\b)"
            r".*\bcopy_symbol_file_targets_to_attributed\s*\("
        ),
        23,
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
                "crates/tsz-lsp/src/completions/member.rs",
                "let mut member_visited = visited.clone();",
            ),
            (
                "crates/tsz-solver/src/evaluation/evaluate_rules/infer_pattern.rs",
                "let mut alias_visited = visited.clone();",
            ),
            (
                "crates/tsz-solver/src/evaluation/evaluate_rules/infer_pattern_helpers.rs",
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


def scan_independent_pipelines(
    search_roots: list[pathlib.Path], max_pipelines: int
) -> list[str]:
    """Count files that construct a full ParserState + BinderState + CheckerState pipeline.

    Workstream 3 exit criterion is "one blessed parse-bind-check path".  Any
    non-test source file under the given roots that calls all three of
    `ParserState::new`, `BinderState::new`, `CheckerState::new` is an
    independent pipeline.  Returns one hit per pipeline file when the total
    exceeds `max_pipelines`, plus a summary line.

    Walks each root directly (does not rely on ROOT-relative paths) so that
    test fixtures under temp dirs can use the same scanner.
    """
    pipeline_files: list[str] = []
    constructors = ("ParserState::new", "BinderState::new", "CheckerState::new")
    for base in search_roots:
        if not base.exists():
            continue
        for path in base.rglob("*.rs"):
            try:
                rel_to_root = path.relative_to(ROOT).as_posix()
            except ValueError:
                # Test fixture under a temp dir — fall back to a path
                # relative to the search root for the report and test_file
                # heuristic.
                rel_to_root = path.relative_to(base).as_posix()
            parts = set(rel_to_root.split("/"))
            if EXCLUDE_DIRS.intersection(parts):
                continue
            if is_test_file(rel_to_root) or "tests" in parts:
                continue
            try:
                text = path.read_text(encoding="utf-8", errors="ignore")
            except OSError:
                continue
            if all(needle in text for needle in constructors):
                pipeline_files.append(rel_to_root)

    pipeline_files.sort()
    if len(pipeline_files) > max_pipelines:
        hits = [
            f"independent pipeline #{i + 1}: {rel}"
            for i, rel in enumerate(pipeline_files)
        ]
        hits.append(
            f"total independent parse-bind-check pipelines: {len(pipeline_files)} "
            f"(cap {max_pipelines}; bump cap intentionally and update ROADMAP.md, "
            f"or consolidate through the compiler service shell — workstream 3)"
        )
        return hits
    return []


_SOLVER_IMPORT_PATTERN = re.compile(
    r"\buse\s+tsz_solver(?:::|\s*;|\s+as\b)|\bextern\s+crate\s+tsz_solver\b"
)

ROOT_SOLVER_COMPUTATION_API_SYMBOLS = (
    "AnyPropagationMode",
    "AnyPropagationRules",
    "are_types_structurally_identical",
    "AssignabilityChecker",
    "BinaryOpEvaluator",
    "BinaryOpResult",
    "CallEvaluator",
    "CallResult",
    "CompatChecker",
    "apply_contextual_type",
    "ContextualTypeContext",
    "evaluate_conditional",
    "evaluate_index_access",
    "evaluate_index_access_with_options",
    "evaluate_keyof",
    "evaluate_mapped",
    "evaluate_type",
    "evaluate_type_with_request",
    "get_contextual_signature",
    "get_contextual_signature_cached",
    "get_contextual_signature_cached_with_compat_checker",
    "get_contextual_signature_for_arity",
    "get_contextual_signature_for_arity_cached",
    "get_contextual_signature_for_arity_cached_with_compat_checker",
    "get_contextual_signature_for_arity_with_compat_checker",
    "get_contextual_signature_with_compat_checker",
    "infer_generic_function",
    "instantiate_function_with_type_args",
    "instantiate_generic",
    "instantiate_type",
    "instantiate_type_cached",
    "instantiate_type_params_to_constraints",
    "instantiate_type_preserving",
    "instantiate_type_preserving_cached",
    "instantiate_type_preserving_meta",
    "instantiate_type_preserving_meta_cached",
    "instantiate_type_with_depth_status",
    "instantiate_type_with_infer",
    "instantiate_type_with_infer_cached",
    "is_subtype_of",
    "rest_argument_element_type",
    "SubtypeChecker",
    "SubtypeResult",
    "substitute_this_type",
    "substitute_this_type_at_return_position",
    "substitute_this_type_cached",
    "TypeEnvironment",
    "TypeEvaluator",
    "TypeInstantiator",
    "TypeResolver",
    "TypeSubstitution",
)

_ROOT_SOLVER_COMPUTATION_IMPORT_PATTERN = re.compile(
    r"\btsz_solver::(?:"
    + "|".join(re.escape(symbol) for symbol in ROOT_SOLVER_COMPUTATION_API_SYMBOLS)
    + r")\b"
    + r"|\buse\s+tsz_solver::\{[^\n}]*\b(?:"
    + "|".join(re.escape(symbol) for symbol in ROOT_SOLVER_COMPUTATION_API_SYMBOLS)
    + r")\b"
)

_QUERY_BOUNDARY_COMMON_REFERENCE_PATTERN = re.compile(
    r"\b(?:crate::)?query_boundaries::common::"
)

# Architecture health metric 5: snapshot-rollback call site count.
#
# Matches broad `CheckerContext` rollback methods, snapshot restorers, and
# `*guard.rollback(` SpeculationGuard calls. `DiagnosticSpeculationSnapshot`
# holder methods are intentionally not counted here because their signatures
# already require the narrow `DiagnosticState` capability.
_SPECULATION_ROLLBACK_PATTERN = re.compile(
    r"\.rollback_full\b"
    r"|\.\s*rollback_diagnostics(?:_filtered)?\b"
    r"|\.rollback_and_replace_diagnostics\b"
    r"|\.rollback_return_type\b"
    r"|\.restore_ts2454_state\b"
    r"|\.restore_implicit_any_closures\b"
    r"|\b\w*guard\.rollback\("
)


def scan_solver_import_count(
    search_roots: list[pathlib.Path],
    exclude_path_prefixes: tuple[str, ...],
    max_imports: int,
) -> list[str]:
    """Count non-test source files that import `tsz_solver` outside the
    solver/checker boundary (architecture health metric 3).

    A file "imports tsz_solver" if a non-comment line matches
    `_SOLVER_IMPORT_PATTERN` (covers `use tsz_solver::`, `pub use tsz_solver`,
    `extern crate tsz_solver`).  Test files (`*_tests.rs`, `test_*.rs`, files
    inside any `tests/` or `benches/` directory) are excluded; only the first
    matching line per file is recorded.  Files whose ROOT-relative path starts
    with any of `exclude_path_prefixes` (e.g. `crates/tsz-solver/`,
    `crates/tsz-checker/`) are skipped — those are the architecturally
    allowed consumers (solver internals + checker boundary modules).

    Returns one hit per offending file when the total exceeds `max_imports`,
    plus a summary line.  Walks each root directly so test fixtures under
    temp dirs can reuse the same scanner via paths relative to the search
    root.
    """
    importing_files: list[str] = []
    for base in search_roots:
        if not base.exists():
            continue
        for path in base.rglob("*.rs"):
            try:
                rel_to_root = path.relative_to(ROOT).as_posix()
            except ValueError:
                # Test fixture under a temp dir — fall back to a path
                # relative to the search root for the report and exclusion
                # heuristics.
                rel_to_root = path.relative_to(base).as_posix()
            parts = set(rel_to_root.split("/"))
            if EXCLUDE_DIRS.intersection(parts):
                continue
            if "tests" in parts or "benches" in parts:
                continue
            if is_test_file(rel_to_root):
                continue
            if any(rel_to_root.startswith(prefix) for prefix in exclude_path_prefixes):
                continue
            try:
                text = path.read_text(encoding="utf-8", errors="ignore")
            except OSError:
                continue
            for line in text.splitlines():
                if line.lstrip().startswith("//"):
                    continue
                if _SOLVER_IMPORT_PATTERN.search(line):
                    importing_files.append(rel_to_root)
                    break

    importing_files.sort()
    if len(importing_files) > max_imports:
        hits = [
            f"direct tsz_solver import #{i + 1}: {rel}"
            for i, rel in enumerate(importing_files)
        ]
        hits.append(
            f"total direct tsz_solver imports outside solver/checker: "
            f"{len(importing_files)} (cap {max_imports}; bump cap intentionally "
            f"and update ROADMAP.md, or route the consumer through the compiler "
            f"service shell or `tsz_checker::query_boundaries` — workstream 3)"
        )
        return hits
    return []


def scan_root_solver_computation_import_count(
    search_roots: list[pathlib.Path],
    exclude_path_prefixes: tuple[str, ...],
    max_references: int,
) -> list[str]:
    """Count flat `tsz_solver` computation API references in production code.

    This ratchets #8204's compatibility debt: the solver crate still exposes
    computation symbols from its root for legacy callers, but new production
    code should route through named facades or the checker query-boundary
    layer. Test files are excluded, and `exclude_path_prefixes` marks approved
    boundary modules such as `crates/tsz-checker/src/query_boundaries/`.
    """
    matching_lines: list[tuple[str, int]] = []
    for base in search_roots:
        if not base.exists():
            continue
        for path in base.rglob("*.rs"):
            try:
                rel_to_root = path.relative_to(ROOT).as_posix()
            except ValueError:
                rel_to_root = path.relative_to(base).as_posix()
            parts = set(rel_to_root.split("/"))
            if EXCLUDE_DIRS.intersection(parts):
                continue
            if "tests" in parts or "benches" in parts:
                continue
            if is_test_file(rel_to_root):
                continue
            if any(rel_to_root.startswith(prefix) for prefix in exclude_path_prefixes):
                continue
            try:
                text = path.read_text(encoding="utf-8", errors="ignore")
            except OSError:
                continue
            for line_no, line in enumerate(text.splitlines(), start=1):
                if line.lstrip().startswith("//"):
                    continue
                if _ROOT_SOLVER_COMPUTATION_IMPORT_PATTERN.search(line):
                    matching_lines.append((rel_to_root, line_no))

    matching_lines.sort()
    if len(matching_lines) > max_references:
        hits = [
            f"flat solver computation API reference #{i + 1}: {rel}:{line_no}"
            for i, (rel, line_no) in enumerate(matching_lines)
        ]
        hits.append(
            f"total flat root solver computation API references outside "
            f"query boundaries: {len(matching_lines)} (cap {max_references}; "
            f"bump cap intentionally, or route the new site through a named "
            f"solver facade / checker query-boundary helper — #8204)"
        )
        return hits
    return []


def _iter_root_pub_use_statements(text: str) -> Iterable[tuple[int, str]]:
    """Yield top-level `pub use` statements from a Rust module text.

    The solver `lib.rs` compatibility exports are plain top-level statements.
    Nested tiered-module re-exports are intentionally ignored because their
    lines are indented and represent the preferred facade modules.
    """
    lines = text.splitlines()
    index = 0
    while index < len(lines):
        line = lines[index]
        if not line.startswith("pub use "):
            index += 1
            continue

        start_line = index + 1
        statement_lines = [line.strip()]
        while ";" not in lines[index] and index + 1 < len(lines):
            index += 1
            statement_lines.append(lines[index].strip())
        yield start_line, " ".join(statement_lines)
        index += 1


def _root_pub_use_export_names(statement: str) -> list[str]:
    """Return exported leaf names from a top-level `pub use` statement."""
    body = statement.removeprefix("pub use ").removesuffix(";").strip()
    if "*" in body:
        return ["*"]

    if "{" not in body:
        return [body.rsplit("::", 1)[-1].strip()]

    inner = body.split("{", 1)[1].rsplit("}", 1)[0]
    names: list[str] = []
    for part in inner.split(","):
        item = part.strip()
        if not item:
            continue
        item = item.split(" as ", 1)[0].strip()
        names.append(item.rsplit("::", 1)[-1])
    return names


def scan_solver_root_explicit_reexport_count(
    file_path: pathlib.Path,
    root_module_prefixes: tuple[str, ...],
    max_reexports: int,
) -> list[str]:
    """Count explicit high-risk solver root compatibility re-exports.

    #8204 is retiring the flat `tsz_solver` root surface in favor of tiered
    facades. Wildcard exports are already forbidden; this guard pins the
    remaining explicit root exports from computation/construction-heavy
    modules so new compatibility surface must be an intentional cap change.
    """
    if not file_path.exists():
        return []

    try:
        text = file_path.read_text(encoding="utf-8", errors="ignore")
    except OSError:
        return []

    prefix_set = set(root_module_prefixes)
    matches: list[tuple[int, str]] = []
    for line_no, statement in _iter_root_pub_use_statements(text):
        body = statement.removeprefix("pub use ").strip()
        root_prefix = body.split("::", 1)[0]
        if root_prefix not in prefix_set:
            continue
        for name in _root_pub_use_export_names(statement):
            matches.append((line_no, name))

    if len(matches) > max_reexports:
        try:
            rel_path = file_path.relative_to(ROOT).as_posix()
        except ValueError:
            rel_path = str(file_path)
        hits = [
            f"flat solver root explicit re-export #{i + 1}: {rel_path}:{line_no} {name}"
            for i, (line_no, name) in enumerate(matches)
        ]
        hits.append(
            f"total flat solver root explicit computation re-exports: "
            f"{len(matches)} (cap {max_reexports}; bump cap intentionally, "
            f"or move the API behind a named facade / checker query-boundary "
            f"helper — #8204)"
        )
        return hits
    return []


def scan_query_boundary_common_reference_count(
    search_roots: list[pathlib.Path],
    exclude_path_prefixes: tuple[str, ...],
    max_references: int,
) -> list[str]:
    """Count direct references to the broad `query_boundaries::common` barrel.

    #8225 tracks turning `query_boundaries` from migration quarantine into
    request-shaped APIs. Existing checker callers are tolerated as baseline
    debt, but new production references outside `query_boundaries/` should use
    a narrower boundary module or deliberately update this cap.
    """
    matching_lines: list[tuple[str, int]] = []
    for base in search_roots:
        if not base.exists():
            continue
        paths = [base] if base.is_file() else base.rglob("*.rs")
        for path in paths:
            try:
                rel_to_root = path.relative_to(ROOT).as_posix()
            except ValueError:
                rel_to_root = path.relative_to(base).as_posix()
            parts = set(rel_to_root.split("/"))
            if EXCLUDE_DIRS.intersection(parts):
                continue
            if "tests" in parts or "benches" in parts:
                continue
            if is_test_file(rel_to_root):
                continue
            if any(rel_to_root.startswith(prefix) for prefix in exclude_path_prefixes):
                continue
            try:
                text = path.read_text(encoding="utf-8", errors="ignore")
            except OSError:
                continue
            for line_no, line in enumerate(text.splitlines(), start=1):
                if line.lstrip().startswith("//"):
                    continue
                if _QUERY_BOUNDARY_COMMON_REFERENCE_PATTERN.search(line):
                    matching_lines.append((rel_to_root, line_no))

    matching_lines.sort()
    if len(matching_lines) > max_references:
        hits = [
            f"direct query_boundaries::common reference #{i + 1}: {rel}:{line_no}"
            for i, (rel, line_no) in enumerate(matching_lines)
        ]
        hits.append(
            f"total direct query_boundaries::common references outside "
            f"query_boundaries: {len(matching_lines)} (cap {max_references}; "
            f"bump cap intentionally, or route the new site through a narrower "
            f"request-shaped boundary - #8225)"
        )
        return hits
    return []


_QUERY_BOUNDARY_ALLOWANCE_TOKEN_PATTERN = re.compile(
    r"\b(?:dead_code|private_interfaces|clippy::[A-Za-z0-9_]+)\b"
)

_CLIPPY_ALLOW_ATTR_PATTERN = re.compile(
    r"#!?\[(?:allow|expect)\([^)\n]*clippy::"
)


def scan_query_boundary_module_allowance_count(
    file_path: pathlib.Path,
    max_allowances: int,
) -> list[str]:
    """Count root-level lint allowance entries in `query_boundaries/mod.rs`.

    The module map historically carried broad `#[allow(...)]` blocks for many
    boundary modules. Existing suppressions are tolerated as migration debt, but
    new blanket entries should be scoped to the item that needs them or ratchet
    this cap intentionally.
    """
    try:
        text = file_path.read_text(encoding="utf-8", errors="ignore")
    except OSError:
        return []

    matches: list[tuple[int, str]] = []
    for line_no, line in enumerate(text.splitlines(), start=1):
        stripped = line.lstrip()
        if stripped.startswith("//"):
            continue
        for match in _QUERY_BOUNDARY_ALLOWANCE_TOKEN_PATTERN.finditer(line):
            matches.append((line_no, match.group(0)))

    if len(matches) > max_allowances:
        try:
            rel_path = file_path.relative_to(ROOT).as_posix()
        except ValueError:
            rel_path = str(file_path)
        hits = [
            f"query_boundaries module lint allowance #{i + 1}: "
            f"{rel_path}:{line_no} {name}"
            for i, (line_no, name) in enumerate(matches)
        ]
        hits.append(
            f"total query_boundaries module-level lint allowance entries: "
            f"{len(matches)} (cap {max_allowances}; scope the allowance to the "
            f"item that needs it or intentionally bump the #8225 cap)"
        )
        return hits
    return []


def scan_workspace_clippy_allow_count(
    search_roots: list[pathlib.Path],
    max_count: int,
) -> list[str]:
    """Count Clippy suppression attribute lines workspace-wide (#9446).

    Covers `#[allow(clippy::...)]`, `#![allow(clippy::...)]`, and
    `#[expect(clippy::...)]`, including test files.  Each cleanup PR must lower
    the cap; any PR adding a new suppression must intentionally bump it with an
    explanation.
    """
    matching: list[tuple[str, int]] = []
    for base in search_roots:
        if not base.exists():
            continue
        for path in base.rglob("*.rs"):
            try:
                rel = path.relative_to(ROOT).as_posix()
            except ValueError:
                rel = path.relative_to(base).as_posix()
            parts = set(rel.split("/"))
            if EXCLUDE_DIRS.intersection(parts):
                continue
            try:
                text = path.read_text(encoding="utf-8", errors="ignore")
            except OSError:
                continue
            for line_no, line in enumerate(text.splitlines(), start=1):
                if line.lstrip().startswith("//"):
                    continue
                if _CLIPPY_ALLOW_ATTR_PATTERN.search(line):
                    matching.append((rel, line_no))

    if len(matching) > max_count:
        matching.sort()
        hits = [
            f"clippy suppression #{i + 1}: {rel}:{line_no}"
            for i, (rel, line_no) in enumerate(matching)
        ]
        hits.append(
            f"total Clippy suppression attribute lines: {len(matching)} "
            f"(cap {max_count}; lower the cap when removing a suppression, "
            f"or bump it intentionally and explain why — #9446)"
        )
        return hits
    return []


def scan_snapshot_rollback_file_count(
    search_roots: list[pathlib.Path],
    exclude_path_prefixes: tuple[str, ...],
    max_files: int,
) -> list[str]:
    """Count non-test files that call any speculation-rollback API outside
    `crates/tsz-checker/src/context/speculation.rs` (architecture health
    metric 5: snapshot-restore call sites).

    A file "calls a broad speculation-rollback API" if a non-comment line
    matches `_SPECULATION_ROLLBACK_PATTERN` (covers `CheckerContext`
    rollback methods, the `restore_ts2454_state` /
    `restore_implicit_any_closures` snapshot restorers, and
    `*guard.rollback(` SpeculationGuard calls). `DiagnosticSpeculationSnapshot`
    holder calls are intentionally outside this metric because they already
    consume the narrow `DiagnosticState` capability. Test files
    (`*_tests.rs`, `test_*.rs`, files inside any `tests/` or `benches/`
    directory) are excluded; only the first matching line per file is
    recorded.  Files whose ROOT-relative path starts with any of
    `exclude_path_prefixes` (e.g. `crates/tsz-checker/src/context/speculation.rs`)
    are skipped — that is the canonical home of the rollback API surface.

    Returns one hit per offending file when the total exceeds `max_files`,
    plus a summary line.  Walks each root directly so test fixtures under
    temp dirs can reuse the same scanner via paths relative to the search
    root.
    """
    rollback_files: list[str] = []
    for base in search_roots:
        if not base.exists():
            continue
        for path in base.rglob("*.rs"):
            try:
                rel_to_root = path.relative_to(ROOT).as_posix()
            except ValueError:
                # Test fixture under a temp dir — fall back to a path
                # relative to the search root for the report and exclusion
                # heuristics.
                rel_to_root = path.relative_to(base).as_posix()
            parts = set(rel_to_root.split("/"))
            if EXCLUDE_DIRS.intersection(parts):
                continue
            if "tests" in parts or "benches" in parts:
                continue
            if is_test_file(rel_to_root):
                continue
            if any(rel_to_root.startswith(prefix) for prefix in exclude_path_prefixes):
                continue
            try:
                text = path.read_text(encoding="utf-8", errors="ignore")
            except OSError:
                continue
            for line in text.splitlines():
                if line.lstrip().startswith("//"):
                    continue
                if _SPECULATION_ROLLBACK_PATTERN.search(line):
                    rollback_files.append(rel_to_root)
                    break

    rollback_files.sort()
    if len(rollback_files) > max_files:
        hits = [
            f"snapshot-rollback caller #{i + 1}: {rel}"
            for i, rel in enumerate(rollback_files)
        ]
        hits.append(
            f"total snapshot-rollback caller files outside speculation.rs: "
            f"{len(rollback_files)} (cap {max_files}; bump cap intentionally "
            f"and update ROADMAP.md, or fold the new call site into an "
            f"existing speculation guard pattern — workstream 4)"
        )
        return hits
    return []


_LSP_FEATURE_METHOD_PATTERN = re.compile(
    r"^[ \t]+pub\s+(?:async\s+)?fn\s+"
    r"(get_|provide_|prepare_|handle_|on_|find_|resolve_)\w+\s*[<(]"
)


def scan_lsp_feature_method_count(
    path: pathlib.Path, max_methods: int
) -> list[str]:
    """Count LSP feature-dispatch methods in `project/features.rs` and
    report when over `max_methods` (architecture health metric 7).

    A "feature-dispatch method" is any indented `pub fn` (optionally
    `pub async fn`) whose name begins with one of the LSP request-handler
    verbs: `get_`, `provide_`, `prepare_`, `handle_`, `on_`, `find_`,
    `resolve_`. The leading indent excludes the matching prefix from
    appearing on a top-level free function or in a doc-comment example.

    Comment lines (`//`, `///`, `//!`) are skipped before pattern
    matching so doc examples that show the method shape don't get
    double-counted.

    Workstream 6 ("LSP And WASM As Service Clients") exit criterion 3
    wants LSP request handling to mostly map protocol inputs to service
    queries; the raw count makes drift visible — each new dispatch
    method must bump the cap with a ROADMAP entry, consolidation onto
    a service trait shows up as a cap reduction.
    """
    if not path.exists():
        return []

    try:
        text = path.read_text(encoding="utf-8", errors="ignore")
    except OSError:
        return []

    method_lines: list[tuple[int, str]] = []
    for line_no, line in enumerate(text.splitlines(), start=1):
        stripped = line.lstrip()
        if stripped.startswith("//"):
            continue
        if _LSP_FEATURE_METHOD_PATTERN.match(line):
            # Capture the function name for the report.
            m = re.search(r"\bfn\s+(\w+)", line)
            if m:
                method_lines.append((line_no, m.group(1)))

    if len(method_lines) > max_methods:
        try:
            rel_path = path.relative_to(ROOT).as_posix()
        except ValueError:
            rel_path = str(path)
        hits = [
            f"LSP feature method #{i + 1}: {rel_path}:{line_no} {name}"
            for i, (line_no, name) in enumerate(method_lines)
        ]
        hits.append(
            f"total LSP feature-dispatch methods in {rel_path}: "
            f"{len(method_lines)} (cap {max_methods}; bump cap intentionally "
            f"and update ROADMAP.md, or consolidate onto a service trait "
            f"surface — workstream 6)"
        )
        return hits
    return []


_SPECULATION_GUARD_STRUCT_PATTERN = re.compile(
    r"^[ \t]*pub(?:\([^)]*\))?\s+struct\s+(\w*Guard\w*)\b"
)


def scan_speculation_guard_struct_count(
    path: pathlib.Path, max_guard_count: int
) -> list[str]:
    """Count `…Guard` struct declarations in the speculation file and
    report when over `max_guard_count` (architecture health metric 6).

    Architecture health metric 6 ("Speculation APIs with surprising
    non-RAII behavior", `docs/plan/ROADMAP.md`) was originally violated
    by `DiagnosticSpeculationGuard`, whose name implied RAII rollback
    while the implementation did implicit-commit-on-drop. PR #1213
    renamed it to `DiagnosticSpeculationSnapshot`. This guard pins
    the rename: any new `pub(crate) struct …Guard` re-introduces the
    same ambiguity.

    The match runs against the literal speculation file
    (`crates/tsz-checker/src/context/speculation.rs`); doc-comment
    references like `SpeculationGuard` in narrative text don't match
    because the regex requires the `pub … struct …` prefix on a non-
    comment line.
    """
    if not path.exists():
        return []

    try:
        text = path.read_text(encoding="utf-8", errors="ignore")
    except OSError:
        return []

    guard_lines: list[tuple[int, str]] = []
    for line_no, line in enumerate(text.splitlines(), start=1):
        stripped = line.lstrip()
        if stripped.startswith("//"):
            continue
        m = _SPECULATION_GUARD_STRUCT_PATTERN.match(line)
        if m:
            guard_lines.append((line_no, m.group(1)))

    if len(guard_lines) > max_guard_count:
        try:
            rel_path = path.relative_to(ROOT).as_posix()
        except ValueError:
            rel_path = str(path)
        hits = [
            f"speculation `…Guard` struct: {rel_path}:{line_no} {name}"
            for line_no, name in guard_lines
        ]
        hits.append(
            f"total `…Guard` structs in {rel_path}: {len(guard_lines)} "
            f"(cap {max_guard_count}; rename to `…Snapshot` to match "
            f"the speculation surface's actual implicit-commit-on-drop "
            f"semantics — workstream-4 Speculation Policy 3 / "
            f"`docs/architecture/ROBUSTNESS_AUDIT_2026-04-26.md` item 5)"
        )
        return hits
    return []


_DEBUG_PRINT_REPORT_MODULE = None


def _load_debug_print_report_module():
    """Load the debug-print scanner that owns comment/string parsing."""
    global _DEBUG_PRINT_REPORT_MODULE
    if _DEBUG_PRINT_REPORT_MODULE is not None:
        return _DEBUG_PRINT_REPORT_MODULE
    spec = importlib.util.spec_from_file_location(
        "debug_print_report", DEBUG_PRINT_REPORT_PATH
    )
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load {DEBUG_PRINT_REPORT_PATH}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    _DEBUG_PRINT_REPORT_MODULE = module
    return module


def scan_debug_print_macros(root: pathlib.Path, scan_dirs: tuple[str, ...]) -> list[str]:
    """Report non-test compiler-internal `println!`, `eprintln!`, and `dbg!`."""
    report = _load_debug_print_report_module()
    hits = report.scan(root, scan_dirs)
    return [f"{hit.path}:{hit.line} {hit.macro}: {hit.text}" for hit in hits]


def extract_js_array_strings(text: str, const_name: str) -> Optional[list[str]]:
    """Extract string literals from a simple JS `NAME = [...]` array."""
    match = re.search(
        rf"\b(?:export\s+)?const\s+{re.escape(const_name)}\s*=\s*\[(?P<body>.*?)\]\s*;",
        text,
        re.DOTALL,
    )
    if match is None:
        return None
    return re.findall(r'"([^"]+)"', match.group("body"))


def extract_project_dashboard_row_names(text: str) -> Optional[list[str]]:
    """Extract `name` fields from `COMPATIBILITY_CORPUS_ROWS` objects."""
    match = re.search(
        r"\b(?:export\s+)?const\s+COMPATIBILITY_CORPUS_ROWS\s*=\s*\[(?P<body>.*?)\]\s*;",
        text,
        re.DOTALL,
    )
    if match is None:
        return None
    return re.findall(r'\bname:\s*"([^"]+)"', match.group("body"))


def extract_project_row_definitions(text: str) -> Optional[list[dict[str, Optional[str]]]]:
    """Extract project row metadata from `PROJECT_ROW_DEFINITIONS`.

    The architecture guard intentionally stays lightweight and avoids executing
    project scripts. This parser only reads the scalar fields needed by the
    Track 1 drift checks.
    """
    match = re.search(
        r"\b(?:export\s+)?const\s+PROJECT_ROW_DEFINITIONS\s*=\s*\[(?P<body>.*?)\]\s*;",
        text,
        re.DOTALL,
    )
    if match is None:
        return None

    rows: list[dict[str, Optional[str]]] = []
    for object_match in re.finditer(r"\{(?P<object>.*?)\}", match.group("body"), re.DOTALL):
        body = object_match.group("object")
        name_match = re.search(r'\bname:\s*"([^"]+)"', body)
        if name_match is None:
            continue

        row: dict[str, Optional[str]] = {"name": name_match.group(1)}
        for field in ("benchmark_set", "guard_set"):
            field_match = re.search(rf'\b{field}:\s*(?:"([^"]+)"|null)', body)
            if field_match is None:
                row[field] = None
            else:
                row[field] = field_match.group(1)
        rows.append(row)

    return rows


def project_rows_by_field(
    definitions: Optional[list[dict[str, Optional[str]]]],
    field: str,
    value: str,
) -> list[str]:
    if definitions is None:
        return []
    return [row["name"] for row in definitions if row.get(field) == value and row.get("name")]


def project_row_names(definitions: Optional[list[dict[str, Optional[str]]]]) -> list[str]:
    if definitions is None:
        return []
    return [row["name"] for row in definitions if row.get("name")]


def project_required_rows(
    text: str,
    definitions: Optional[list[dict[str, Optional[str]]]],
) -> Optional[list[str]]:
    rows = extract_js_array_strings(text, "REQUIRED_PROJECT_ROWS")
    if rows is not None:
        return rows
    return project_rows_by_field(definitions, "benchmark_set", "required") or None


def project_compile_canary_rows(
    text: str,
    definitions: Optional[list[dict[str, Optional[str]]]],
) -> Optional[list[str]]:
    rows = extract_js_array_strings(text, "COMPILE_CANARY_PROJECT_ROWS")
    if rows is not None:
        return rows
    return project_rows_by_field(definitions, "guard_set", "canary") or None


def scan_project_dashboard_rows(path: pathlib.Path) -> list[str]:
    """Ensure every expected project benchmark row is present in the dashboard.

    `scripts/bench/project-rows.mjs` owns the shared row inventories:

    - `REQUIRED_PROJECT_ROWS` / `COMPILE_CANARY_PROJECT_ROWS` define the
      project rows that must exist as benchmark/CI compatibility records.
    - `COMPATIBILITY_CORPUS_ROWS` defines the rows rendered by the public
      project compatibility dashboard.

    Track 1 requires public rows for every required project. This guard keeps
    the render inventory in lockstep with the expected benchmark/canary row
    inventories without hard-coding the project names here.
    """
    if not path.exists():
        return [f"{relative_path(path)}:0 benchmark data file is missing"]

    text = path.read_text(encoding="utf-8", errors="ignore")
    definitions = extract_project_row_definitions(text)
    expected = project_required_rows(text, definitions)
    canary = project_compile_canary_rows(text, definitions)
    dashboard = extract_project_dashboard_row_names(text)
    if dashboard is None:
        dashboard = project_row_names(definitions) or None
    rel = relative_path(path)
    hits: list[str] = []

    if expected is None:
        hits.append(f"{rel}:0 missing REQUIRED_PROJECT_ROWS array")
        expected = []
    if canary is None:
        hits.append(f"{rel}:0 missing COMPILE_CANARY_PROJECT_ROWS array")
        canary = []
    if dashboard is None:
        hits.append(f"{rel}:0 missing COMPATIBILITY_CORPUS_ROWS array")
        dashboard = []

    required = sorted(set(expected) | set(canary))
    dashboard_set = set(dashboard)
    required_set = set(required)

    for name in required:
        if name not in dashboard_set:
            hits.append(f"{rel}:0 missing compatibility dashboard row for {name}")

    for name in sorted(dashboard_set - required_set):
        hits.append(f"{rel}:0 stale compatibility dashboard row for {name}")

    duplicates = sorted({name for name in dashboard if dashboard.count(name) > 1})
    for name in duplicates:
        hits.append(f"{rel}:0 duplicate compatibility dashboard row for {name}")

    return hits


def extract_project_fixture_source_case_names(text: str) -> Optional[list[str]]:
    """Extract row names handled by `tsz_project_fixture_sources`."""
    match = re.search(
        r"\btsz_project_fixture_sources\s*\(\)\s*\{(?P<body>.*?)^\}",
        text,
        re.DOTALL | re.MULTILINE,
    )
    if match is None:
        return None

    names: list[str] = []
    for line in match.group("body").splitlines():
        case_match = re.match(r"^\s*([A-Za-z0-9_.-]+(?:\|[A-Za-z0-9_.-]+)*)\)\s*$", line)
        if case_match is None:
            continue
        names.extend(case_match.group(1).split("|"))
    return names


def emitted_project_fixture_sources(
    fixture_path: pathlib.Path,
    row_name: str,
) -> tuple[list[tuple[str, str, str]], Optional[str]]:
    """Run `tsz_project_fixture_sources` and validate emitted metadata lines."""
    command = (
        "set -euo pipefail; "
        f"source {shlex.quote(str(fixture_path))}; "
        f"tsz_project_fixture_sources {shlex.quote(row_name)}"
    )
    try:
        result = subprocess.run(
            ["bash", "-c", command],
            cwd=ROOT,
            text=True,
            capture_output=True,
            timeout=10,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        return [], f"could not run fixture source metadata for {row_name}: {exc}"

    if result.returncode != 0:
        detail = (result.stderr or result.stdout).strip()
        suffix = f": {detail}" if detail else ""
        return [], f"could not run fixture source metadata for {row_name}{suffix}"

    sources: list[tuple[str, str, str]] = []
    for line_number, raw_line in enumerate(result.stdout.splitlines(), start=1):
        line = raw_line.strip()
        if not line:
            continue
        fields = [field.strip() for field in line.split("|")]
        if len(fields) != 3 or any(field == "" for field in fields):
            return (
                [],
                f"malformed fixture source metadata for {row_name} line {line_number}: {line}",
            )
        sources.append((fields[0], fields[1], fields[2]))

    if not sources:
        return [], f"empty fixture source metadata for {row_name}"

    return sources, None


def scan_project_fixture_sources(
    row_path: pathlib.Path,
    fixture_path: pathlib.Path,
) -> list[str]:
    """Ensure pinned project rows have fixture source/ref metadata.

    `scripts/bench/project-rows.mjs` owns the public project row inventory.
    `scripts/bench/project-fixtures.sh` owns the pinned external fixture refs
    and exposes `tsz_project_fixture_sources` so benchmark/CI compatibility
    rows carry reproducibility metadata.
    """
    hits: list[str] = []
    row_rel = relative_path(row_path)
    fixture_rel = relative_path(fixture_path)

    if not row_path.exists():
        return [f"{row_rel}:0 project row manifest is missing"]
    if not fixture_path.exists():
        return [f"{fixture_rel}:0 project fixture metadata file is missing"]

    row_text = row_path.read_text(encoding="utf-8", errors="ignore")
    definitions = extract_project_row_definitions(row_text)
    required = project_required_rows(row_text, definitions)
    canary = project_compile_canary_rows(row_text, definitions)
    if required is None:
        hits.append(f"{row_rel}:0 missing REQUIRED_PROJECT_ROWS array")
        required = []
    if canary is None:
        hits.append(f"{row_rel}:0 missing COMPILE_CANARY_PROJECT_ROWS array")
        canary = []

    fixture_text = fixture_path.read_text(encoding="utf-8", errors="ignore")
    source_cases = extract_project_fixture_source_case_names(fixture_text)
    if source_cases is None:
        hits.append(f"{fixture_rel}:0 missing tsz_project_fixture_sources function")
        source_cases = []

    registered_rows = set(required) | set(canary)
    expected_rows = sorted(registered_rows - GENERATED_PROJECT_ROWS_WITHOUT_PINNED_SOURCE)
    expected_set = set(expected_rows)
    source_set = set(source_cases)

    for name in expected_rows:
        if name not in source_set:
            hits.append(f"{fixture_rel}:0 missing fixture source metadata for {name}")
            continue
        _, error = emitted_project_fixture_sources(fixture_path, name)
        if error is not None:
            hits.append(f"{fixture_rel}:0 {error}")

    for name in sorted(source_set - registered_rows):
        hits.append(f"{fixture_rel}:0 stale fixture source metadata for {name}")

    duplicates = sorted({name for name in source_cases if source_cases.count(name) > 1})
    for name in duplicates:
        hits.append(f"{fixture_rel}:0 duplicate fixture source metadata for {name}")

    return hits


def extract_project_compile_guard_rows(text: str) -> list[str]:
    """Extract row names routed through `should_check_project`."""
    return [
        name
        for name in re.findall(r'\bshould_check_project\s+"([^"]+)"', text)
        if not name.startswith("$")
    ]


def extract_project_benchmark_rows(text: str) -> list[str]:
    """Extract project row names registered in the benchmark runner."""
    return re.findall(r'\brun_isolated\s+"([^"]+)"\s+run_[A-Za-z0-9_]+_benchmarks', text)


def scan_project_inclusion_policy(
    row_path: pathlib.Path,
    compile_guard_path: pathlib.Path,
    bench_path: pathlib.Path,
) -> list[str]:
    """Ensure project row inventories match the shell inclusion policies."""
    hits: list[str] = []
    row_rel = relative_path(row_path)
    compile_rel = relative_path(compile_guard_path)
    bench_rel = relative_path(bench_path)

    if not row_path.exists():
        return [f"{row_rel}:0 project row manifest is missing"]
    if not compile_guard_path.exists():
        return [f"{compile_rel}:0 project compile guard is missing"]
    if not bench_path.exists():
        return [f"{bench_rel}:0 benchmark runner is missing"]

    row_text = row_path.read_text(encoding="utf-8", errors="ignore")
    definitions = extract_project_row_definitions(row_text)
    required = project_required_rows(row_text, definitions)
    canary = project_compile_canary_rows(row_text, definitions)
    if required is None:
        hits.append(f"{row_rel}:0 missing REQUIRED_PROJECT_ROWS array")
        required = []
    if canary is None:
        hits.append(f"{row_rel}:0 missing COMPILE_CANARY_PROJECT_ROWS array")
        canary = []

    manifest_rows = sorted(set(required) | set(canary))

    compile_text = compile_guard_path.read_text(encoding="utf-8", errors="ignore")
    compile_rows = extract_project_compile_guard_rows(compile_text)
    if 'for name in "${TSZ_COMPILE_GUARD_REQUIRED_ROWS[@]}"' in compile_text:
        compile_rows.extend(project_rows_by_field(definitions, "guard_set", "required"))
    if 'for name in "${TSZ_COMPILE_GUARD_CANARY_ROWS[@]}"' in compile_text:
        compile_rows.extend(project_rows_by_field(definitions, "guard_set", "canary"))
    compile_set = set(compile_rows)
    manifest_set = set(manifest_rows)
    expected_compile_rows = sorted(set(manifest_rows) - BENCHMARK_ONLY_PROJECT_ROWS)
    expected_compile_set = set(expected_compile_rows)
    for name in expected_compile_rows:
        if name not in compile_set:
            hits.append(f"{compile_rel}:0 missing project compile guard inclusion for {name}")
    for name in sorted(compile_set - expected_compile_set):
        hits.append(f"{compile_rel}:0 stale project compile guard inclusion for {name}")

    bench_text = bench_path.read_text(encoding="utf-8", errors="ignore")
    bench_rows = extract_project_benchmark_rows(bench_text)
    bench_set = set(bench_rows)
    expected_bench_rows = sorted(manifest_set - COMPILE_GUARD_ONLY_PROJECT_ROWS)
    expected_bench_set = set(expected_bench_rows)
    for name in expected_bench_rows:
        if name not in bench_set:
            hits.append(f"{bench_rel}:0 missing project benchmark inclusion for {name}")

    return hits


def scan_project_config_writers(
    fixture_path: pathlib.Path,
    compile_guard_path: pathlib.Path,
    bench_path: pathlib.Path,
) -> list[str]:
    """Ensure shared project rows use shared config writer functions."""
    hits: list[str] = []
    fixture_rel = relative_path(fixture_path)
    compile_rel = relative_path(compile_guard_path)
    bench_rel = relative_path(bench_path)

    if not fixture_path.exists():
        return [f"{fixture_rel}:0 project fixture metadata file is missing"]
    if not compile_guard_path.exists():
        return [f"{compile_rel}:0 project compile guard is missing"]
    if not bench_path.exists():
        return [f"{bench_rel}:0 benchmark runner is missing"]

    fixture_text = fixture_path.read_text(encoding="utf-8", errors="ignore")
    compile_text = compile_guard_path.read_text(encoding="utf-8", errors="ignore")
    bench_text = bench_path.read_text(encoding="utf-8", errors="ignore")

    for row, writer in sorted(PROJECT_CONFIG_WRITERS.items()):
        if not re.search(rf"\b{re.escape(writer)}\s*\(\)", fixture_text):
            hits.append(f"{fixture_rel}:0 missing shared config writer {writer} for {row}")

        if row not in BENCHMARK_ONLY_PROJECT_ROWS and not re.search(
            rf"\b{re.escape(writer)}\b",
            compile_text,
        ):
            hits.append(f"{compile_rel}:0 {row} does not use shared config writer {writer}")

        if row not in COMPILE_GUARD_ONLY_PROJECT_ROWS and not re.search(
            rf"\b{re.escape(writer)}\b",
            bench_text,
        ):
            hits.append(f"{bench_rel}:0 {row} does not use shared config writer {writer}")

    return hits


def scan_regex_line_count(
    search_roots: list[pathlib.Path],
    pattern: re.Pattern[str],
    max_lines: int,
) -> list[str]:
    """Count non-test, non-comment source lines matching `pattern`.

    This is for Track 10 count ratchets where current architecture debt is
    tolerated but must not grow.  It returns one hit per matching line plus a
    final summary only when the live count exceeds `max_lines`.
    """
    matching_lines: list[tuple[str, int]] = []
    for base in search_roots:
        if not base.exists():
            continue
        paths = [base] if base.is_file() else base.rglob("*.rs")
        for path in paths:
            try:
                rel_to_root = path.relative_to(ROOT).as_posix()
            except ValueError:
                rel_to_root = path.name if base.is_file() else path.relative_to(base).as_posix()
            parts = set(rel_to_root.split("/"))
            if EXCLUDE_DIRS.intersection(parts):
                continue
            if "tests" in parts or "benches" in parts:
                continue
            if is_test_file(rel_to_root):
                continue
            try:
                text = path.read_text(encoding="utf-8", errors="ignore")
            except OSError:
                continue
            for line_no, line in enumerate(text.splitlines(), start=1):
                if line.lstrip().startswith("//"):
                    continue
                if pattern.search(line):
                    matching_lines.append((rel_to_root, line_no))

    matching_lines.sort()
    if len(matching_lines) > max_lines:
        hits = [
            f"matching line #{i + 1}: {rel}:{line_no}"
            for i, (rel, line_no) in enumerate(matching_lines)
        ]
        hits.append(
            f"total matching lines: {len(matching_lines)} "
            f"(cap {max_lines}; bump cap intentionally and update ROADMAP.md, "
            f"or replace the new site with structural facts — Track 10)"
        )
        return hits
    return []


VISITED_CLONE_PATTERN = re.compile(r"\bvisited\.clone\s*\(")


def scan_branch_local_visited_clones(
    search_roots: list[pathlib.Path],
    allowlist: tuple[tuple[str, str], ...],
) -> list[str]:
    """Report new branch-local `visited.clone()` traversal sites.

    The allowlist key is `(relative path, stripped line)`, counted with
    multiplicity.  That keeps this guard stable across nearby line edits while
    still catching duplicate clone branches in an existing file.
    """
    allowed_counts = Counter(allowlist)
    seen_counts: Counter[tuple[str, str]] = Counter()
    hits: list[str] = []

    for base in search_roots:
        if not base.exists():
            continue
        for path in base.rglob("*.rs"):
            try:
                rel_to_root = path.relative_to(ROOT).as_posix()
            except ValueError:
                rel_to_root = path.relative_to(base).as_posix()
            parts = set(rel_to_root.split("/"))
            if EXCLUDE_DIRS.intersection(parts):
                continue
            if "tests" in parts or "benches" in parts:
                continue
            if is_test_file(rel_to_root):
                continue
            try:
                text = path.read_text(encoding="utf-8", errors="ignore")
            except OSError:
                continue
            for line_no, line in enumerate(text.splitlines(), start=1):
                stripped = line.strip()
                if stripped.startswith("//"):
                    continue
                if not VISITED_CLONE_PATTERN.search(stripped):
                    continue

                key = (rel_to_root, stripped)
                seen_counts[key] += 1
                if seen_counts[key] <= allowed_counts[key]:
                    continue
                hits.append(
                    f"{rel_to_root}:{line_no} new branch-local visited.clone() "
                    "traversal site; use memoized DP/worklists/SCCs/bitsets "
                    "or extend the Track 10 allowlist intentionally"
                )

    return hits


def scan_struct_field_count(
    path: pathlib.Path, struct_name: str, max_fields: int
) -> list[str]:
    """Count fields in `pub struct <struct_name>` and report when over `max_fields`.

    Field counting is intentionally regex-based (not syn/AST): the goal is a
    cheap, repeatable arch metric, not a perfect reflection.  Lines that look
    like a field declaration (`name: Type,`) inside the struct body are
    counted; doc comments, empty lines, and `}` terminators are skipped.
    Comments are stripped first via `strip_rust_comments` so commented-out
    fields don't inflate the count.
    """
    if not path.exists():
        return []
    rel = relative_path(path)
    body = find_struct_body(path, struct_name)
    if body is None:
        return [f"{rel}:0 struct {struct_name!r} not found"]

    field_count = len(extract_struct_field_names_from_body(body))

    if field_count > max_fields:
        return [
            f"{rel}:struct {struct_name} has {field_count} fields "
            f"(cap {max_fields}; bump cap intentionally and update ROADMAP.md)"
        ]
    return []


def scan_trait_method_count(
    path: pathlib.Path, trait_name: str, max_methods: int
) -> list[str]:
    """Count method declarations in `pub trait <trait_name>`.

    This is a cheap architecture metric for broad capability traits.  It counts
    every `fn name...` declaration in the trait body, including default-method
    bodies, because both expand the capability surface exposed to algorithms.
    Comments are stripped first so doc examples or commented-out signatures do
    not affect the ratchet.
    """
    if not path.exists():
        return []
    rel = relative_path(path)
    body = find_trait_body(path, trait_name)
    if body is None:
        return [f"{rel}:0 trait {trait_name!r} not found"]

    method_count = len(extract_trait_method_names_from_body(body))

    if method_count > max_methods:
        return [
            f"{rel}:trait {trait_name} has {method_count} methods "
            f"(cap {max_methods}; split onto a narrower trait or bump cap "
            f"intentionally and update #8205)"
        ]
    return []


def relative_path(path: pathlib.Path) -> str:
    try:
        return path.relative_to(ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def find_struct_body(path: pathlib.Path, struct_name: str):
    text = path.read_text(encoding="utf-8", errors="ignore")
    stripped = strip_rust_comments(text)
    header_pattern = re.compile(
        rf"\bpub\s+struct\s+{re.escape(struct_name)}\b[^{{]*\{{",
        re.MULTILINE,
    )
    match = header_pattern.search(stripped)
    if match is None:
        return None

    body_start = match.end()
    depth = 1
    body_end = body_start
    for i in range(body_start, len(stripped)):
        ch = stripped[i]
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                body_end = i
                break
    return stripped[body_start:body_end]


def find_trait_body(path: pathlib.Path, trait_name: str):
    text = path.read_text(encoding="utf-8", errors="ignore")
    stripped = strip_rust_comments(text)
    header_pattern = re.compile(
        rf"\bpub\s+trait\s+{re.escape(trait_name)}\b[^{{]*\{{",
        re.MULTILINE,
    )
    match = header_pattern.search(stripped)
    if match is None:
        return None

    body_start = match.end()
    depth = 1
    body_end = body_start
    for i in range(body_start, len(stripped)):
        ch = stripped[i]
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                body_end = i
                break
    return stripped[body_start:body_end]


STRUCT_FIELD_PATTERN = re.compile(
    r"^\s*(?:pub(?:\s*\([^)]*\))?\s+)?(?P<name>[a-z_][a-zA-Z0-9_]*)\s*:"
)

TRAIT_METHOD_PATTERN = re.compile(
    r"^\s*(?:async\s+|unsafe\s+|const\s+)?fn\s+"
    r"(?P<name>[a-zA-Z_][a-zA-Z0-9_]*)\s*[<(]"
)


def extract_struct_field_names_from_body(body: str) -> list[str]:
    names = []
    for line in body.splitlines():
        match = STRUCT_FIELD_PATTERN.match(line)
        if match:
            names.append(match.group("name"))
    return names


def extract_trait_method_names_from_body(body: str) -> list[str]:
    names = []
    for line in body.splitlines():
        match = TRAIT_METHOD_PATTERN.match(line)
        if match:
            names.append(match.group("name"))
    return names


def extract_struct_field_names(path: pathlib.Path, struct_name: str) -> list[str]:
    if not path.exists():
        return []
    body = find_struct_body(path, struct_name)
    if body is None:
        return []
    return extract_struct_field_names_from_body(body)


def parse_checker_context_lifetime_manifest(
    path: pathlib.Path,
) -> tuple[dict[str, dict[str, object]], list[str]]:
    rel = relative_path(path)
    if not path.exists():
        return {}, [f"{rel}:0 lifetime manifest is missing"]

    entries: dict[str, dict[str, object]] = {}
    errors: list[str] = []
    current = None
    section_pattern = re.compile(r"^\s*\[([A-Za-z_][A-Za-z0-9_]*)\]\s*(?:#.*)?$")
    inline_entry_pattern = re.compile(
        r'^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*\{\s*'
        r'lifetime\s*=\s*"([^"]*)"\s*,\s*'
        r'capability\s*=\s*"([^"]*)"\s*,\s*'
        r'reason\s*=\s*"([^"]*)"\s*'
        r'\}\s*(?:#.*)?$'
    )
    key_value_pattern = re.compile(
        r'^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*"([^"]*)"\s*(?:#.*)?$'
    )

    for line_no, line in enumerate(
        path.read_text(encoding="utf-8", errors="ignore").splitlines(),
        start=1,
    ):
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue

        inline_entry_match = inline_entry_pattern.match(line)
        if inline_entry_match and current is None:
            field, lifetime, capability, reason = inline_entry_match.groups()
            if field in entries:
                errors.append(f"{rel}:{line_no} duplicate manifest entry [{field}]")
            else:
                entries[field] = {
                    "line": line_no,
                    "lifetime": lifetime,
                    "capability": capability,
                    "reason": reason,
                }
            continue

        section_match = section_pattern.match(line)
        if section_match:
            current = section_match.group(1)
            if current in entries:
                errors.append(f"{rel}:{line_no} duplicate manifest section [{current}]")
            else:
                entries[current] = {"line": line_no}
            continue

        key_value_match = key_value_pattern.match(line)
        if key_value_match and current is not None:
            key, value = key_value_match.groups()
            entries[current][key] = value
            continue

        if key_value_match:
            errors.append(f"{rel}:{line_no} key/value entry appears before any section")
        else:
            errors.append(f"{rel}:{line_no} unsupported manifest line")

    return entries, errors


def scan_checker_context_lifetime_manifest(
    struct_path: pathlib.Path,
    struct_name: str,
    manifest_path: pathlib.Path,
) -> list[str]:
    if not struct_path.exists():
        return []
    struct_rel = relative_path(struct_path)
    manifest_rel = relative_path(manifest_path)
    body = find_struct_body(struct_path, struct_name)
    if body is None:
        return [f"{struct_rel}:0 struct {struct_name!r} not found"]

    fields = extract_struct_field_names_from_body(body)
    field_set = set(fields)
    entries, hits = parse_checker_context_lifetime_manifest(manifest_path)
    entry_set = set(entries.keys())

    for field in fields:
        if field not in entries:
            hits.append(
                f"{manifest_rel}:0 missing CheckerContext lifetime for field [{field}]"
            )

    for field in sorted(entry_set - field_set):
        line = entries[field].get("line", 0)
        hits.append(
            f"{manifest_rel}:{line} stale manifest entry [{field}] "
            f"not found in {struct_name}"
        )

    for field, entry in sorted(
        entries.items(), key=lambda item: item[1].get("line", 0)
    ):
        line = entry.get("line", 0)
        lifetime = entry.get("lifetime")
        capability = entry.get("capability")
        reason = entry.get("reason")
        if lifetime is None:
            hits.append(f"{manifest_rel}:{line} [{field}] missing lifetime")
        elif lifetime == "Unknown":
            hits.append(f"{manifest_rel}:{line} [{field}] lifetime must not be Unknown")
        elif lifetime not in VALID_CHECKER_CONTEXT_LIFETIMES:
            hits.append(
                f"{manifest_rel}:{line} [{field}] invalid lifetime {lifetime!r}"
            )
        if capability is None:
            hits.append(f"{manifest_rel}:{line} [{field}] missing capability")
        elif capability == "Unknown":
            hits.append(f"{manifest_rel}:{line} [{field}] capability must not be Unknown")
        elif capability not in VALID_CHECKER_CONTEXT_CAPABILITIES:
            hits.append(
                f"{manifest_rel}:{line} [{field}] invalid capability {capability!r}"
            )
        if not isinstance(reason, str) or not reason.strip():
            hits.append(f"{manifest_rel}:{line} [{field}] missing reason")

    return hits


def escape_markdown_cell(value: object) -> str:
    return str(value).replace("|", "\\|").replace("\n", " ")


def checker_context_lifetime_markdown(
    struct_path: pathlib.Path,
    struct_name: str,
    manifest_path: pathlib.Path,
) -> str:
    fields = extract_struct_field_names(struct_path, struct_name)
    entries, _errors = parse_checker_context_lifetime_manifest(manifest_path)
    lines = [
        "| Field | Lifetime | Capability | Reason |",
        "| --- | --- | --- | --- |",
    ]
    for field in fields:
        entry = entries.get(field, {})
        lifetime = escape_markdown_cell(entry.get("lifetime", "MISSING"))
        capability = escape_markdown_cell(entry.get("capability", "MISSING"))
        reason = escape_markdown_cell(entry.get("reason", "MISSING"))
        lines.append(f"| `{field}` | `{lifetime}` | `{capability}` | {reason} |")
    return "\n".join(lines)


def scan_file_line_limit(path: pathlib.Path, limit: int):
    if not path.exists():
        return []

    try:
        rel = path.relative_to(ROOT).as_posix()
    except ValueError:
        rel = path.as_posix()

    line_count = 0
    try:
        with path.open("r", encoding="utf-8", errors="ignore") as handle:
            for line_count, _line in enumerate(handle, start=1):
                pass
    except OSError:
        return []

    if line_count > limit:
        return [f"{rel}:{line_count} lines (limit {limit})"]
    return []


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
    parser.add_argument(
        "--checker-context-lifetime-table",
        action="store_true",
        help="Print the CheckerContext lifetime manifest as a markdown table.",
    )
    args = parser.parse_args()

    if args.checker_context_lifetime_table:
        name, struct_path, struct_name, manifest_path = (
            CHECKER_CONTEXT_LIFETIME_MANIFEST_CHECKS[0]
        )
        hits = scan_checker_context_lifetime_manifest(
            struct_path, struct_name, manifest_path
        )
        if hits:
            print(name)
            for hit in hits:
                print(f"  {hit}")
            return 1
        print(checker_context_lifetime_markdown(struct_path, struct_name, manifest_path))
        return 0

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

    for name, base, limit, *rest in LINE_LIMIT_CHECKS:
        if not base.exists():
            continue
        exclude_files = rest[0] if rest else None
        hits = scan_line_limits(base, limit, exclude_files)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, path, limit in FILE_LINE_LIMIT_CHECKS:
        hits = scan_file_line_limit(path, limit)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, path, struct_name, max_fields in STRUCT_FIELD_COUNT_CHECKS:
        hits = scan_struct_field_count(path, struct_name, max_fields)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, path, trait_name, max_methods in TRAIT_METHOD_COUNT_CHECKS:
        hits = scan_trait_method_count(path, trait_name, max_methods)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for (
        name,
        struct_path,
        struct_name,
        manifest_path,
    ) in CHECKER_CONTEXT_LIFETIME_MANIFEST_CHECKS:
        hits = scan_checker_context_lifetime_manifest(
            struct_path, struct_name, manifest_path
        )
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, search_roots, max_pipelines in INDEPENDENT_PIPELINE_CHECKS:
        hits = scan_independent_pipelines(search_roots, max_pipelines)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for (
        name,
        search_roots,
        exclude_path_prefixes,
        max_imports,
    ) in SOLVER_IMPORT_COUNT_CHECKS:
        hits = scan_solver_import_count(
            search_roots, exclude_path_prefixes, max_imports
        )
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for (
        name,
        search_roots,
        exclude_path_prefixes,
        max_references,
    ) in ROOT_SOLVER_COMPUTATION_IMPORT_COUNT_CHECKS:
        hits = scan_root_solver_computation_import_count(
            search_roots, exclude_path_prefixes, max_references
        )
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for (
        name,
        file_path,
        root_module_prefixes,
        max_reexports,
    ) in ROOT_SOLVER_EXPLICIT_REEXPORT_COUNT_CHECKS:
        hits = scan_solver_root_explicit_reexport_count(
            file_path, root_module_prefixes, max_reexports
        )
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for (
        name,
        search_roots,
        exclude_path_prefixes,
        max_references,
    ) in QUERY_BOUNDARY_COMMON_REFERENCE_COUNT_CHECKS:
        hits = scan_query_boundary_common_reference_count(
            search_roots, exclude_path_prefixes, max_references
        )
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for (
        name,
        file_path,
        max_allowances,
    ) in QUERY_BOUNDARY_MODULE_ALLOWANCE_COUNT_CHECKS:
        hits = scan_query_boundary_module_allowance_count(file_path, max_allowances)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for (
        name,
        search_roots,
        max_count,
    ) in WORKSPACE_CLIPPY_ALLOW_COUNT_CHECKS:
        hits = scan_workspace_clippy_allow_count(search_roots, max_count)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for (
        name,
        search_roots,
        exclude_path_prefixes,
        max_files,
    ) in SNAPSHOT_ROLLBACK_FILE_COUNT_CHECKS:
        hits = scan_snapshot_rollback_file_count(
            search_roots, exclude_path_prefixes, max_files
        )
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, file_path, max_methods in LSP_FEATURE_METHOD_COUNT_CHECKS:
        hits = scan_lsp_feature_method_count(file_path, max_methods)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, file_path, max_guard_count in SPECULATION_GUARD_NAME_CHECKS:
        hits = scan_speculation_guard_struct_count(file_path, max_guard_count)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, root, scan_dirs in DEBUG_PRINT_MACRO_CHECKS:
        hits = scan_debug_print_macros(root, scan_dirs)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, file_path in PROJECT_DASHBOARD_ROW_CHECKS:
        hits = scan_project_dashboard_rows(file_path)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, row_path, fixture_path in PROJECT_FIXTURE_SOURCE_CHECKS:
        hits = scan_project_fixture_sources(row_path, fixture_path)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, row_path, compile_guard_path, bench_path in PROJECT_INCLUSION_POLICY_CHECKS:
        hits = scan_project_inclusion_policy(row_path, compile_guard_path, bench_path)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, fixture_path, compile_guard_path, bench_path in PROJECT_CONFIG_WRITER_CHECKS:
        hits = scan_project_config_writers(fixture_path, compile_guard_path, bench_path)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, search_roots, pattern, max_lines in REGEX_LINE_COUNT_CHECKS:
        hits = scan_regex_line_count(search_roots, pattern, max_lines)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, search_roots, allowlist in BRANCH_LOCAL_VISITED_CLONE_CHECKS:
        hits = scan_branch_local_visited_clones(search_roots, allowlist)
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
