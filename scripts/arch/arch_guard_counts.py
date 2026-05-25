import pathlib
import re
from collections import Counter
from typing import Iterable

from arch_guard_shared import EXCLUDE_DIRS, ROOT, is_test_file

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
