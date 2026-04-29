#!/usr/bin/env python3
import pathlib
import re
import argparse
import json
import sys
from pathlib import Path

ROOT = pathlib.Path(__file__).resolve().parents[2]

CHECKS = [
    (
        "Production code must not branch on conformance fixture identity",
        ROOT / "crates",
        re.compile(
            r"\bTSZ_CONFORMANCE_TEST\b"
            r"|\bconformance_test_name\b"
            r"|\btest_path\.contains\s*\("
            r"|False Positive Suppressions"
        ),
        {
            "exclude_dirs": {"conformance", "tests"},
            "exclude_test_files": True,
            "ignore_comment_lines": True,
        },
    ),
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
        {
            "exclude_dirs": {"query_boundaries", "tests"},
            "exclude_files": {
                # These files use .lookup() in tracing::trace! macros for debug output only
                "crates/tsz-checker/src/types/computation/complex.rs",
                # Pre-existing: class member lookup in class_checker
                "crates/tsz-checker/src/classes/class_checker.rs",
                "crates/tsz-checker/src/classes/class_checker_compat.rs",
                # Pre-existing: property access lookup
                "crates/tsz-checker/src/checkers/property_checker.rs",
                "crates/tsz-checker/src/state/state_checking/property.rs",
                # Pre-existing: type computation access lookup
                "crates/tsz-checker/src/types/computation/access.rs",
                # Pre-existing baseline debt
                "crates/tsz-checker/src/types/property_access_type/resolve.rs",
                "crates/tsz-checker/src/types/class_type/core.rs",
            },
        },
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
        {
            "exclude_dirs": {"tests", "jsdoc"},
        },
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
        {
            "exclude_dirs": {"tests"},
            "exclude_files": {
                # Pre-existing baseline debt
                "crates/tsz-checker/src/query_boundaries/class.rs",
                "crates/tsz-checker/src/query_boundaries/property_access.rs",
            },
        },
    ),
    (
        "Checker boundary: ObjectFlags must not be imported (use ObjectShape builder methods)",
        ROOT / "crates" / "tsz-checker" / "src",
        re.compile(r"\buse\s+tsz_solver::.*ObjectFlags\b|\bObjectFlags::"),
        {
            "exclude_dirs": {"tests"},
            "ignore_comment_lines": True,
            # Pre-existing: type_environment uses ObjectFlags for const enum checks
            # namespace_checker creates enum namespace objects with ENUM_NAMESPACE flag
            "exclude_files": {
                "crates/tsz-checker/src/state/type_environment/core.rs",
                "crates/tsz-checker/src/declarations/namespace_checker.rs",
                # Pre-existing baseline debt
                "crates/tsz-checker/src/query_boundaries/common.rs",
                "crates/tsz-checker/src/types/property_access_augmentation.rs",
                "crates/tsz-checker/src/types/class_type/core.rs",
            },
        },
    ),
    (
        "Checker boundary: direct solver relation queries outside query boundaries/tests",
        ROOT / "crates" / "tsz-checker",
        re.compile(r"\btsz_solver::(is_subtype_of|is_assignable_to)\s*\("),
        {
            "exclude_dirs": {"query_boundaries", "tests"},
            "exclude_files": set(),
            "ignore_comment_lines": True,
        },
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
    # union2/intersection2 are semantically equivalent to union()/intersection()
    # — just optimized two-argument versions. They are part of the public solver
    # TypeDatabase API and safe to use from the checker.
    # (
    #     "Checker boundary: deprecated two-arg intersection/union constructors",
    #     ROOT / "crates" / "tsz-checker",
    #     re.compile(r"\.intersection2\s*\(|\.union2\s*\("),
    #     {"exclude_dirs": {"tests"}},
    # ),
    (
        "Code quality: no bare .unwrap() in checker production code (use .expect())",
        ROOT / "crates" / "tsz-checker" / "src",
        re.compile(r"\.unwrap\(\)"),
        {
            "exclude_dirs": {"tests"},
            "ignore_comment_lines": True,
            "exclude_test_files": True,  # Skip *_tests.rs files
        },
    ),
    (
        "Code quality: no bare .unwrap() in solver production code (use .expect())",
        ROOT / "crates" / "tsz-solver" / "src",
        re.compile(r"\.unwrap\(\)"),
        {
            "exclude_dirs": {"tests"},
            "ignore_comment_lines": True,
            "exclude_test_files": True,
            # Inline #[cfg(test)] modules at the bottom of these files
            "exclude_files": {
                "crates/tsz-solver/src/type_queries/data.rs",
                "crates/tsz-solver/src/type_queries/flow.rs",
                # Inline/adjacent test modules under src/
                "crates/tsz-solver/src/type_queries/data/tests.rs",
            },
        },
    ),
    (
        "Code quality: no bare .unwrap() in binder production code (use .expect())",
        ROOT / "crates" / "tsz-binder" / "src",
        re.compile(r"\.unwrap\(\)"),
        {
            "exclude_dirs": {"tests"},
            "ignore_comment_lines": True,
            "exclude_test_files": True,
            # state/tests.rs is a #[path = "tests.rs"] test module
            "exclude_files": {
                "crates/tsz-binder/src/state/tests.rs",
            },
        },
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
        {"exclude_dirs": {"tests"}, "ignore_comment_lines": True},
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
        {
            "exclude_dirs": {"tests"},
            "exclude_files": {
                # Pre-existing baseline debt
                "crates/tsz-emitter/src/declaration_emitter/helpers/mod.rs",
                "crates/tsz-emitter/src/declaration_emitter/helpers/type_printing.rs",
            },
        },
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
            "exclude_dirs": {"tsz-cli", "tsz-core", "conformance", "tests"},
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
            "exclude_dirs": {"tsz-cli", "tsz-core", "conformance", "tests"},
            "ignore_comment_lines": True,
        },
    ),
    (
        "Non-solver/non-lowering crates must not inspect TypeData internals in production code",
        ROOT / "crates",
        re.compile(r"\buse\s+tsz_solver::.*TypeData\b|\bTypeData::"),
        {
            "exclude_dirs": {"tsz-solver", "tsz-lowering", "tsz-core", "tests"},
            "exclude_files": {
                # query_boundaries is the canonical boundary layer — TypeData
                # matching here is intentional and architecturally correct.
                "crates/tsz-checker/src/query_boundaries/state/type_environment.rs",
                "crates/tsz-checker/src/query_boundaries/class.rs",
                # Pre-existing baseline debt
                "crates/tsz-checker/src/types/class_type/core.rs",
                "crates/tsz-emitter/src/declaration_emitter/helpers/mod.rs",
                "crates/tsz-emitter/src/declaration_emitter/helpers/type_printing.rs",
                "crates/tsz-lsp/src/signature_help.rs",
            },
            "ignore_comment_lines": True,
        },
    ),
    (
        "Core boundary: wasm bindings must stay in current wasm surface files",
        ROOT / "crates" / "tsz-core" / "src",
        re.compile(r"\bwasm_bindgen\b|\bserde_wasm_bindgen\b|\bJsValue\b"),
        {
            "exclude_dirs": {"tests"},
            "exclude_files": {
                # Transitional baseline: core lib exports the wasm API today.
                "crates/tsz-core/src/lib.rs",
                # Explicit wasm API module surface.
                "crates/tsz-core/src/api/wasm/code_actions.rs",
                "crates/tsz-core/src/api/wasm/compiler_options.rs",
                "crates/tsz-core/src/api/wasm/core_utils.rs",
                "crates/tsz-core/src/api/wasm/parser.rs",
                "crates/tsz-core/src/api/wasm/program.rs",
                "crates/tsz-core/src/api/wasm/program_results.rs",
                "crates/tsz-core/src/api/wasm/transforms.rs",
            },
            "ignore_comment_lines": True,
        },
    ),
    (
        "LSP boundary: direct lookup() on solver interner",
        ROOT / "crates" / "tsz-lsp",
        re.compile(r"\.lookup\s*\("),
        {"exclude_dirs": {"tests"}, "exclude_files": {
            # file_id_allocator.lookup() is not a solver interner lookup
            "crates/tsz-lsp/src/project/core.rs",
            # Pre-existing baseline debt
            "crates/tsz-lsp/src/signature_help.rs",
        }},
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
            },
            "ignore_comment_lines": True,
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
        # Files removed from exclusion after dropping below 2000 lines:
        # jsx_checker.rs (1985), complex.rs (1907→1123), type_resolution/core.rs (1018),
        # variable_checking/core.rs (1689), dispatch.rs (1981), type_node.rs (1997),
        # computed.rs (1925), property_access_type.rs (1808), duplicate_identifiers.rs (1911),
        # type_analysis/core.rs (1816), error_reporter/core.rs (1766),
        # statement_callback_bridge.rs (1965), type_node.rs (1699),
        # assignment_checker.rs (1721), assignability_checker.rs (1223),
        # enum_utils.rs (1695), helpers.rs (1475), class_type/core.rs (1729),
        # object_literal.rs (1977), access.rs (1449), property.rs (1337),
        # jsx_checker.rs (removed), state/state.rs (1787)
        {
            "crates/tsz-checker/src/types/function_type.rs",
            "crates/tsz-checker/src/state/type_analysis/computed_helpers.rs",
            "crates/tsz-checker/src/types/computation/call.rs",
            "crates/tsz-checker/src/tests/architecture_contract_tests.rs",
            "crates/tsz-checker/src/types/type_checking/core.rs",
            "crates/tsz-checker/src/dispatch.rs",
            "crates/tsz-checker/src/tests/dispatch_tests.rs",
            "crates/tsz-checker/src/checkers/call_checker.rs",
            "crates/tsz-checker/src/checkers/jsx/props.rs",
            "crates/tsz-checker/src/error_reporter/assignability.rs",
            "crates/tsz-checker/src/flow/control_flow/assignment.rs",
            "crates/tsz-checker/src/symbols/symbol_resolver.rs",
            "crates/tsz-checker/src/state/type_environment/core.rs",
            "crates/tsz-checker/src/types/computation/identifier.rs",
            "crates/tsz-checker/src/types/queries/core.rs",
            "crates/tsz-checker/src/flow/flow_analysis/usage.rs",
            # Pre-existing: recently grew past 2000 lines
            "crates/tsz-checker/src/types/interface_type.rs",
            "crates/tsz-checker/src/state/state_checking_members/statement_callback_bridge.rs",
            "crates/tsz-checker/src/context/mod.rs",
            # Recently grew past 2000 lines (CI lint blocker on origin/main)
            "crates/tsz-checker/src/context/core.rs",
            "crates/tsz-checker/src/flow/control_flow/condition_narrowing.rs",
            "crates/tsz-checker/src/declarations/import/core/import_members.rs",
            # Pre-existing oversized files captured as the current ratchet baseline.
            "crates/tsz-checker/src/checkers/generic_checker.rs",
            "crates/tsz-checker/src/types/property_access_helpers.rs",
            "crates/tsz-checker/src/types/utilities/core.rs",
            "crates/tsz-checker/src/types/computation/binary.rs",
            "crates/tsz-checker/src/types/computation/object_literal.rs",
            "crates/tsz-checker/src/classes/class_implements_checker.rs",
            "crates/tsz-checker/src/declarations/import/core.rs",
            "crates/tsz-checker/src/state/variable_checking/core.rs",
            "crates/tsz-checker/src/state/variable_checking/variable_helpers.rs",
            "crates/tsz-checker/src/state/type_analysis/computed_commonjs.rs",
            "crates/tsz-checker/src/state/type_analysis/computed.rs",
            "crates/tsz-checker/src/jsdoc/resolution.rs",
            "crates/tsz-checker/src/assignability/assignment_checker.rs",
            "crates/tsz-checker/src/error_reporter/call_errors.rs",
            "crates/tsz-checker/src/flow/control_flow/core.rs",
            # Pre-existing oversized files captured as current ratchet baseline.
            "crates/tsz-checker/src/classes/class_checker.rs",
            "crates/tsz-checker/src/jsdoc/params.rs",
            "crates/tsz-checker/src/symbols/scope_finder.rs",
            "crates/tsz-checker/src/assignability/assignability_checker.rs",
            "crates/tsz-checker/src/error_reporter/render_failure.rs",
            "crates/tsz-checker/src/state/type_resolution/module.rs",
            "crates/tsz-checker/src/state/variable_checking/destructuring.rs",
            "crates/tsz-checker/src/state/state_checking/class.rs",
            "crates/tsz-checker/src/state/type_analysis/core.rs",
            "crates/tsz-checker/src/declarations/import/declaration.rs",
            "crates/tsz-checker/src/types/type_checking/duplicate_identifiers.rs",
            "crates/tsz-checker/src/types/type_checking/duplicate_identifiers_helpers.rs",
            "crates/tsz-checker/src/types/property_access_type/resolve.rs",
            "crates/tsz-checker/src/types/queries/lib.rs",
            "crates/tsz-checker/src/types/computation/call_inference.rs",
            "crates/tsz-checker/src/types/class_type/core.rs",
            "crates/tsz-checker/src/types/class_type/constructor.rs",
            "crates/tsz-checker/src/types/computation/object_literal/computation.rs",
            "crates/tsz-checker/src/types/computation/call/inner.rs",
            # Pre-existing: grew past 2000 lines from assignment-ops refactor
            "crates/tsz-checker/src/assignability/assignment_checker/assignment_ops.rs",
            # Pre-existing oversized files captured as current ratchet baseline.
            "crates/tsz-checker/src/state/state_checking_members/interface_checks.rs",
            "crates/tsz-checker/src/jsdoc/diagnostics.rs",
            "crates/tsz-checker/src/error_reporter/properties.rs",
            "crates/tsz-checker/src/error_reporter/core/diagnostic_source.rs",
            "crates/tsz-checker/src/types/utilities/enum_utils.rs",
            # Pre-existing: checker context module aggregates project-wide state.
            "crates/tsz-checker/src/context/mod.rs",
            # Bumped by the LiteralKeyof inference fix: the keyof-display
            # branch in `contextual_keyof_parameter_display` now consults
            # `query_common::type_has_displayable_name` so anonymous shapes
            # fall through to the printer's eager `keyof { ... }` evaluation.
            "crates/tsz-checker/src/error_reporter/call_errors/elaboration.rs",
            # Pre-existing: display_formatting.rs grew past 2000 raw lines
            # (LOC ~1823, under the CI threshold; local raw-line guard catches it).
            "crates/tsz-checker/src/error_reporter/call_errors/display_formatting.rs",
            # Pre-existing: context/core.rs is the project-wide state container; grew
            # past 2000 raw lines through ongoing checker boundary work.
            # Pre-existing: condition_narrowing.rs hosts the dispatch table for
            # discriminant/literal/typeof narrowing arms; grew past 2000 raw lines.
        },
    ),
]

FILE_LINE_LIMIT_CHECKS = [
    (
        "Core boundary: tsz-core lib facade must stay under 2420 LOC",
        ROOT / "crates" / "tsz-core" / "src" / "lib.rs",
        2420,
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
        223,
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
# solver/checker boundary (architecture health metric 3 in
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
        "Frontend/emitter boundary: direct tsz_solver imports outside solver/checker (architecture health metric 3)",
        [ROOT / "crates"],
        (
            "crates/tsz-solver/",
            "crates/tsz-checker/",
        ),
        37,
    ),
]

SNAPSHOT_ROLLBACK_FILE_COUNT_CHECKS = [
    (
        "Checker speculation boundary: snapshot-rollback call sites outside speculation.rs (architecture health metric 5)",
        [ROOT / "crates" / "tsz-checker" / "src"],
        ("crates/tsz-checker/src/context/speculation.rs",),
        15,
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

EXCLUDE_DIRS = {".git", "target", "node_modules"}
SOLVER_TYPEDATA_QUARANTINE_ALLOWLIST = {
    "crates/tsz-solver/src/intern/mod.rs",
    "crates/tsz-solver/src/intern/core.rs",
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

# Architecture health metric 5: snapshot-rollback call site count.
#
# Matches CheckerContext rollback methods, snapshot restorers, and
# `*guard.rollback(` SpeculationGuard calls. The `\w*guard\.rollback\(`
# alternative requires "guard" (with optional prefix) immediately before the
# method to avoid catching unrelated `.rollback(` methods on other types.
_SPECULATION_ROLLBACK_PATTERN = re.compile(
    r"\.rollback_full\b"
    r"|\.rollback_diagnostics(?:_filtered)?\b"
    r"|\.rollback_and_replace_diagnostics\b"
    r"|\.rollback_return_type\b"
    r"|\.rollback_filtered\b"
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


def scan_snapshot_rollback_file_count(
    search_roots: list[pathlib.Path],
    exclude_path_prefixes: tuple[str, ...],
    max_files: int,
) -> list[str]:
    """Count non-test files that call any speculation-rollback API outside
    `crates/tsz-checker/src/context/speculation.rs` (architecture health
    metric 5: snapshot-restore call sites).

    A file "calls a speculation-rollback API" if a non-comment line matches
    `_SPECULATION_ROLLBACK_PATTERN` (covers `CheckerContext::rollback_*`,
    the `restore_ts2454_state` / `restore_implicit_any_closures` snapshot
    restorers, and `*guard.rollback(` SpeculationGuard calls).  Test files
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
    try:
        rel = path.relative_to(ROOT).as_posix()
    except ValueError:
        rel = path.as_posix()

    text = path.read_text(encoding="utf-8", errors="ignore")
    stripped = strip_rust_comments(text)

    # Find `pub struct <struct_name><...generic args...> {` and the matching
    # closing brace via depth counting (not regex over braces, which would
    # miss nested types like `FxHashMap<K, V>` inside fields).
    header_pattern = re.compile(
        rf"\bpub\s+struct\s+{re.escape(struct_name)}\b[^{{]*\{{",
        re.MULTILINE,
    )
    match = header_pattern.search(stripped)
    if match is None:
        return [f"{rel}:0 struct {struct_name!r} not found"]

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
    body = stripped[body_start:body_end]

    field_pattern = re.compile(r"^\s*(?:pub(?:\s*\([^)]*\))?\s+)?[a-z_][a-zA-Z0-9_]*\s*:")
    field_count = sum(1 for line in body.splitlines() if field_pattern.match(line))

    if field_count > max_fields:
        return [
            f"{rel}:struct {struct_name} has {field_count} fields "
            f"(cap {max_fields}; bump cap intentionally and update ROADMAP.md)"
        ]
    return []


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
