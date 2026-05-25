import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parents[2]

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
        238,
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
        # Ratcheted to the post-merge count for the Application-source
        # refresh PR after current main reduced the quarantine surface.
        3349,
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
