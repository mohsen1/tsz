"""Per-file physical-line-count ratchets (`FILE_LINE_LIMIT_CHECKS`).

Split out of `arch_guard_shared.py` (#16745): the guard's own implementation
had grown past the 2000-LOC cap it enforces, and this list of one-tuple-per-
file size ratchets was the single largest, most self-contained chunk (over
1000 lines of data, no logic). Moving it here is a pure relocation — no
behavior change — and every name below is re-exported through `arch_guard.py`
exactly as before, so `self.arch_guard.FILE_LINE_LIMIT_CHECKS` and friends in
`test_arch_guard.py` keep working unmodified.
"""

from arch_guard_shared import ROOT

# Ratcheted 1901 -> 1740 by the #15643 arch-health paydown: FunctionShape
# instantiation, parameter-list transformation, and redeclaration-widening
# helpers moved to `generic_instantiation`, `signature_building`, and
# `widening` (parent #8225).
QUERY_BOUNDARY_COMMON_LINE_BASELINE = 1740

# Temporary green-campaign headroom for #14351. The live baseline remains
# explicit; this reserve lets urgent parity PRs land while #8225 follow-up
# slices move helpers out of the broad `common` quarantine again.
QUERY_BOUNDARY_COMMON_LINE_GREEN_HEADROOM = 24

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
        QUERY_BOUNDARY_COMMON_LINE_BASELINE
        + QUERY_BOUNDARY_COMMON_LINE_GREEN_HEADROOM,
    ),
    (
        "Solver diagnostics formatter boundary: format/mod.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-solver"
        / "src"
        / "diagnostics"
        / "format"
        / "mod.rs",
        2012,
    ),
    (
        "Emitter transform boundary: class_es5_ir.rs must not grow",
        ROOT / "crates" / "tsz-emitter" / "src" / "transforms" / "class_es5_ir.rs",
        2101,
    ),
    # The five entries below pin files that crossed 2000 lines while the
    # guard ran in no CI job (#15643). Pinned at their observed size so they
    # cannot grow further; ratchet down as split-out submodules land.
    (
        "Solver type-queries boundary: type_queries/core.rs must not grow",
        ROOT / "crates" / "tsz-solver" / "src" / "type_queries" / "core.rs",
        2174,
    ),
    (
        "Emitter transform boundary: transforms/helpers.rs must not grow",
        ROOT / "crates" / "tsz-emitter" / "src" / "transforms" / "helpers.rs",
        2099,
    ),
    (
        "Emitter transform boundary: class_es5_ir_members.rs must not grow",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "transforms"
        / "class_es5_ir_members.rs",
        2037,
    ),
    (
        "Emitter statements boundary: emitter/statements/core.rs must not grow",
        ROOT / "crates" / "tsz-emitter" / "src" / "emitter" / "statements" / "core.rs",
        2029,
    ),
    (
        "Solver relation-explain boundary: subtype/explain.rs must not grow",
        ROOT / "crates" / "tsz-solver" / "src" / "relations" / "subtype" / "explain.rs",
        2026,
    ),
    (
        "Solver instantiation boundary: instantiate.rs must not grow",
        ROOT / "crates" / "tsz-solver" / "src" / "instantiation" / "instantiate.rs",
        2098,
    ),
    (
        "Solver diagnostics boundary: format/mod.rs must not grow",
        ROOT
        / "crates"
        / "tsz-solver"
        / "src"
        / "diagnostics"
        / "format"
        / "mod.rs",
        2012,
    ),
    (
        "Solver evaluation boundary: conditional.rs must not grow",
        ROOT
        / "crates"
        / "tsz-solver"
        / "src"
        / "evaluation"
        / "evaluate_rules"
        / "conditional.rs",
        2083,
    ),
    (
        "Emitter transform boundary: ir_printer.rs must not grow",
        ROOT / "crates" / "tsz-emitter" / "src" / "transforms" / "ir_printer.rs",
        2003,
    ),
    (
        "Emitter expression boundary: private_fields.rs size ratchet (#8276)",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "emitter"
        / "expressions"
        / "core"
        / "private_fields.rs",
        2006,
    ),
    (
        "Solver engine boundary: generic call resolver must stay at current 3413 LOC baseline (#8209)",
        ROOT
        / "crates"
        / "tsz-solver"
        / "src"
        / "operations"
        / "generic_call"
        / "resolve.rs",
        3413,
    ),
    (
        "Solver generic-call boundary: inference_helpers size ratchet",
        ROOT
        / "crates"
        / "tsz-solver"
        / "src"
        / "operations"
        / "generic_call"
        / "inference_helpers.rs",
        2065,
    ),
    (
        "Solver inference boundary: infer_matching size ratchet",
        ROOT / "crates" / "tsz-solver" / "src" / "inference" / "infer_matching.rs",
        2002,
    ),
    # Pin the async ES5 IR transformer file size while #8277 splits the
    # monolith into staged lowering modules. The cap should ratchet down
    # as more phases (helper scheduling, temp/hoist planning, suspended
    # target lowering, ...) are extracted into sibling submodules.
    # Ratcheted 5150→4918 after submodule extraction reduced the core engine.
    # Ratcheted 4918→4924: +6 lines for catch_binding_ordinals field init.
    (
        "Emitter boundary: async ES5 IR engine size ratchet (#8277)",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "transforms"
        / "async_es5_ir.rs",
        4924,
    ),
    # Emitter ES decorators: PR #10778 tracks sharding into 7 focused submodules.
    # Ratchet down as submodules land.
    (
        "Emitter boundary: es_decorators monolith size ratchet (#10778)",
        ROOT / "crates" / "tsz-emitter" / "src" / "transforms" / "es_decorators.rs",
        1147,
    ),
    # Config monolith: tsconfig/compiler-options parser. Issue #8280 tracks
    # splitting into option-domain submodules. Ratchet down as each domain lands.
    # Ratcheted 8206→4981 after extracting the 3.2k-LOC test module into
    # config/tests/{options_parsing,module_resolution,strict_lib_extends}.rs.
    (
        # Ratcheted 4275→4281: +6 lines for tsconfig selector normalization
        # and config-validation fixes (#12493, #12496).
        "Core boundary: tsconfig/config monolith size ratchet (#8280)",
        ROOT / "crates" / "tsz-core" / "src" / "config" / "mod.rs",
        4281,
    ),
    # LSP signature-help root provider has been split into signature_help/.
    # Keep the largest implementation shard pinned while the remaining
    # TypeData/direct lookup() debt in shapes.rs burns down separately.
    (
        "LSP boundary: signature_help contextual shard size ratchet",
        ROOT / "crates" / "tsz-lsp" / "src" / "signature_help" / "contextual.rs",
        1309,
    ),
    # Scanner main loop: issue #9431 tracks splitting by token family.
    (
        "Scanner boundary: scanner_impl monolith size ratchet (#9431)",
        ROOT / "crates" / "tsz-scanner" / "src" / "scanner_impl.rs",
        1463,
    ),
    # CLI driver resolution: split into discovery/exports_imports/package_resolution/
    # path_resolution/type_packages submodules; ratchet holds the orchestrator at 301.
    (
        "CLI boundary: driver/resolution monolith size ratchet",
        ROOT / "crates" / "tsz-cli" / "src" / "driver" / "resolution.rs",
        301,
    ),
    # Emitter class declarations: split by emit feature family per §19.
    (
        "Emitter boundary: class declaration emitter size ratchet",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "emitter"
        / "declarations"
        / "class"
        / "emit_es6.rs",
        4139,
    ),
    # CLI driver check-utils: ProgramData construction. Issue #9412 tracks
    # extracting the source-resolution phase.
    # Ratcheted 3949→2466 after extracting the test module into
    # driver/check_utils/tests.rs.
    (
        "CLI boundary: driver/check_utils monolith size ratchet (#9412)",
        ROOT / "crates" / "tsz-cli" / "src" / "driver" / "check_utils.rs",
        2466,
    ),
    # LSP module-specifier resolution: split by resolution family per §19.
    (
        "LSP boundary: module_specifiers monolith size ratchet",
        ROOT / "crates" / "tsz-lsp" / "src" / "project" / "module_specifiers.rs",
        3669,
    ),
    # Binder declaration binding: split by declaration family per §19.
    (
        "Binder boundary: binder/declaration monolith size ratchet",
        ROOT / "crates" / "tsz-binder" / "src" / "binding" / "declaration.rs",
        3038,
    ),
    # Emitter class ES5 AST-to-IR: issue #10638 tracks splitting alongside
    # async_es5_ir.rs. Partially split (comments/control-flow/expressions/for-in-of
    # submodules already extracted); ratchet holds orchestrator at 1869.
    (
        "Emitter boundary: class ES5 AST-to-IR engine size ratchet (#10638)",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "transforms"
        / "class_es5_ast_to_ir.rs",
        1869,
    ),
    # CLI LSP server: completions handler — split by completion kind per §19.
    (
        "CLI LSP server: handlers_completions monolith size ratchet",
        ROOT
        / "crates"
        / "tsz-cli"
        / "src"
        / "bin"
        / "tsz_server"
        / "handlers_completions.rs",
        3577,
    ),
    # CLI main binary: split by command family per §19.
    (
        "CLI boundary: tsz main binary size ratchet",
        ROOT / "crates" / "tsz-cli" / "src" / "bin" / "tsz.rs",
        3566,
    ),
    # CLI driver core: orchestrates check/emit/resolve pipeline. Ratchet down
    # as pipeline stages are extracted per §19.
    (
        # Ratcheted 3186→3193: +7 lines for config-validation false-positive
        # fixes (#12493, #12496).
        "CLI boundary: driver/core monolith size ratchet",
        ROOT / "crates" / "tsz-cli" / "src" / "driver" / "core.rs",
        3193,
    ),
    # CLI LSP server: structure/outline handler — split by request kind per §19.
    (
        "CLI LSP server: handlers_structure monolith size ratchet",
        ROOT
        / "crates"
        / "tsz-cli"
        / "src"
        / "bin"
        / "tsz_server"
        / "handlers_structure.rs",
        3075,
    ),
    # CLI LSP server: hover/signature/semantic handler — split by feature per §19.
    (
        "CLI LSP server: handlers_info monolith size ratchet",
        ROOT
        / "crates"
        / "tsz-cli"
        / "src"
        / "bin"
        / "tsz_server"
        / "handlers_info.rs",
        2881,
    ),
    # CLI LSP server: editing/refactor handler — split by action family per §19.
    (
        "CLI LSP server: handlers_editing monolith size ratchet",
        ROOT
        / "crates"
        / "tsz-cli"
        / "src"
        / "bin"
        / "tsz_server"
        / "handlers_editing.rs",
        2332,
    ),
    # LSP project core: orchestrates multi-file state. Ratchet down as file
    # management is delegated to ProjectFileSet/CompilationGroup per §19.
    (
        "LSP boundary: project/core monolith size ratchet",
        ROOT / "crates" / "tsz-lsp" / "src" / "project" / "core.rs",
        2916,
    ),
    # LSP fourslash: language-service test protocol runner. Ratchet down as
    # test helpers are extracted into focused sub-modules per §19.
    (
        "LSP boundary: fourslash test protocol size ratchet",
        ROOT / "crates" / "tsz-lsp" / "src" / "fourslash.rs",
        2268,
    ),
    # Emitter DTS portability resolver: split by portability family per §19.
    (
        "Emitter boundary: declaration_emitter/helpers/portability_resolve size ratchet",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "declaration_emitter"
        / "helpers"
        / "portability_resolve.rs",
        3178,
    ),
    # Emitter DTS type-inference helper: issue #8276 tracks migrating inference
    # output to structured declaration summary facts.
    (
        "Emitter boundary: declaration_emitter/helpers/type_inference size ratchet (#8276)",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "declaration_emitter"
        / "helpers"
        / "type_inference.rs",
        2846,
    ),
    # Emitter using/disposable region: issue #8276 tracks migrating the 16
    # output-surgery rewrites to structured resource-region IR.
    # Ratcheted 2537→2608 here in #12503 because main grew past the prior
    # cap between this branch's base and the synthetic-merge test (issues
    # #12499 / #12492 — this PR's reason for being). The new cap matches
    # the live count on the rebased synthetic merge.
    (
        "Emitter boundary: source_file/top_level_using size ratchet (#8276)",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "emitter"
        / "source_file"
        / "top_level_using.rs",
        2608,
    ),
    # Emitter property/element access: split by access kind per §19.
    # Bumped by 4 (1499->1503) for the ES5 optional-chain `this`-receiver fix:
    # the four leaf downlevel branches (property/element x simple/non-simple)
    # each record `optional_chain_sync_tail_start` so a consuming optional call
    # can splice the captured receiver inside the nullish guard.
    (
        "Emitter boundary: emitter/expressions/access size ratchet",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "emitter"
        / "expressions"
        / "access.rs",
        1503,
    ),
    # --- Blanket coverage batch: all production files > 2000 lines per §19 ---
    # These entries pin the current baseline and prevent silent growth.
    # Each file is a candidate for splitting; ratchet down as submodules land.
    (
        "Solver boundary: narrowing/core.rs size ratchet",
        ROOT / "crates/tsz-solver/src/narrowing/core.rs",
        2655,
    ),
    (
        "Solver boundary: def/resolver.rs size ratchet",
        ROOT / "crates/tsz-solver/src/def/resolver.rs",
        2541,
    ),
    (
        "Solver boundary: evaluate_rules/infer_pattern.rs size ratchet",
        ROOT / "crates/tsz-solver/src/evaluation/evaluate_rules/infer_pattern.rs",
        2343,
    ),
    (
        "Solver boundary: caches/db.rs size ratchet",
        ROOT / "crates/tsz-solver/src/caches/db.rs",
        2334,
    ),
    (
        "Emitter boundary: class_es5_ast_to_ir_expressions.rs size ratchet",
        ROOT / "crates/tsz-emitter/src/transforms/class_es5_ast_to_ir_expressions.rs",
        2223,
    ),
    (
        "Solver boundary: type_queries/data/signatures_and_advanced.rs size ratchet",
        ROOT / "crates/tsz-solver/src/type_queries/data/signatures_and_advanced.rs",
        2191,
    ),
    (
        "CLI boundary: driver/check_tests/check_tests_part1.rs size ratchet",
        ROOT / "crates/tsz-cli/src/driver/check_tests/check_tests_part1.rs",
        2158,
    ),
    (
        "CLI boundary: driver/check.rs size ratchet",
        ROOT / "crates/tsz-cli/src/driver/check.rs",
        2131,
    ),
    (
        "Emitter boundary: emitter/functions.rs size ratchet",
        ROOT / "crates/tsz-emitter/src/emitter/functions.rs",
        2121,
    ),
    (
        "Emitter boundary: emitter/expressions/call.rs size ratchet",
        ROOT / "crates/tsz-emitter/src/emitter/expressions/call.rs",
        2080,
    ),
    (
        "Common boundary: perf_counters/tests.rs size ratchet",
        ROOT / "crates/tsz-common/src/perf_counters/tests.rs",
        2066,
    ),
    (
        "Solver boundary: type_queries/data/content_predicates.rs size ratchet",
        ROOT / "crates/tsz-solver/src/type_queries/data/content_predicates.rs",
        2043,
    ),
    (
        "Solver boundary: operations/widening.rs size ratchet",
        ROOT / "crates/tsz-solver/src/operations/widening.rs",
        2042,
    ),
    (
        "CLI LSP server: main.rs size ratchet",
        ROOT / "crates/tsz-cli/src/bin/tsz_server/main.rs",
        2038,
    ),
    (
        "Solver boundary: evaluation/evaluate/support.rs size ratchet",
        ROOT / "crates/tsz-solver/src/evaluation/evaluate/support.rs",
        2008,
    ),
    (
        "Solver boundary: diagnostics/format/mod.rs size ratchet",
        ROOT / "crates/tsz-solver/src/diagnostics/format/mod.rs",
        2012,
    ),
    (
        "Solver boundary: type_queries/data/tests.rs size ratchet",
        ROOT / "crates/tsz-solver/src/type_queries/data/tests.rs",
        2035,
    ),
    (
        "Solver boundary: intern/core/constructors.rs size ratchet",
        ROOT / "crates/tsz-solver/src/intern/core/constructors.rs",
        2026,
    ),
    (
        "Binder boundary: nodes/binding.rs size ratchet",
        ROOT / "crates/tsz-binder/src/nodes/binding.rs",
        1587,
    ),
    (
        "Solver boundary: diagnostics/format/mod.rs size ratchet",
        ROOT / "crates/tsz-solver/src/diagnostics/format/mod.rs",
        2012,
    ),
    (
        "Emitter boundary: declaration_emitter/helpers/type_inference_return_normalization.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "declaration_emitter"
        / "helpers"
        / "type_inference_return_normalization.rs",
        2006,
    ),
    (
        "Emitter boundary: emitter/expressions/core/private_fields.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "emitter"
        / "expressions"
        / "core"
        / "private_fields.rs",
        2006,
    ),
    (
        "Checker boundary: types/property_access_type/resolve.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "types"
        / "property_access_type"
        / "resolve.rs",
        1035,
    ),
    (
        "Checker boundary: types/type_checking/duplicate_identifiers_helpers.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "types"
        / "type_checking"
        / "duplicate_identifiers_helpers.rs",
        1657,
    ),
    (
        "Checker boundary: error_reporter/properties.rs size ratchet",
        ROOT / "crates" / "tsz-checker" / "src" / "error_reporter" / "properties.rs",
        1897,
    ),
    (
        "Checker boundary: declarations/import/declaration.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "declarations"
        / "import"
        / "declaration.rs",
        429,
    ),
    (
        "Checker boundary: state/state_checking/property.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "state"
        / "state_checking"
        / "property.rs",
        1480,
    ),
    (
        "Parser boundary: parser/state_expressions_literals.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-parser"
        / "src"
        / "parser"
        / "state_expressions_literals.rs",
        3011,
    ),
    (
        "Checker boundary: jsdoc/params.rs size ratchet",
        ROOT / "crates" / "tsz-checker" / "src" / "jsdoc" / "params.rs",
        577,
    ),
    (
        "Checker boundary: types/type_checking/duplicate_identifiers.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "types"
        / "type_checking"
        / "duplicate_identifiers.rs",
        1992,
    ),
    (
        "Checker boundary: flow/control_flow/core.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "flow"
        / "control_flow"
        / "core.rs",
        1848,
    ),
    (
        "Solver boundary: type_queries/flow.rs size ratchet",
        ROOT / "crates" / "tsz-solver" / "src" / "type_queries" / "flow.rs",
        2755,
    ),
    (
        "Checker boundary: types/utilities/core.rs size ratchet",
        ROOT / "crates" / "tsz-checker" / "src" / "types" / "utilities" / "core.rs",
        1703,
    ),
    (
        "Checker boundary: state/type_analysis/core.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "state"
        / "type_analysis"
        / "core.rs",
        1880,
    ),
    (
        # Ratcheted 2736→2896: +160 lines for three config-validation
        # false-positive test cases (#12493).
        "CLI boundary: driver/tests.rs size ratchet",
        ROOT / "crates" / "tsz-cli" / "src" / "driver" / "tests.rs",
        2896,
    ),
    (
        "Checker boundary: types/class_type/constructor.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "types"
        / "class_type"
        / "constructor.rs",
        1981,
    ),
    (
        "Solver boundary: diagnostics/format/compound.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-solver"
        / "src"
        / "diagnostics"
        / "format"
        / "compound.rs",
        458,
    ),
    (
        "Solver boundary: diagnostics/format/mod.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-solver"
        / "src"
        / "diagnostics"
        / "format"
        / "mod.rs",
        2012,
    ),
    (
        "Checker boundary: assignability/assignability_diagnostics.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "assignability"
        / "assignability_diagnostics.rs",
        2539,
    ),
    (
        "Checker boundary: state/type_resolution/module.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "state"
        / "type_resolution"
        / "module.rs",
        1953,
    ),
    (
        "Parser boundary: parser/state_statements_class_members.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-parser"
        / "src"
        / "parser"
        / "state_statements_class_members.rs",
        2587,
    ),
    (
        "Checker boundary: state/type_environment/core.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "state"
        / "type_environment"
        / "core.rs",
        1463,
    ),
    (
        "Conformance boundary: conformance runner size ratchet",
        ROOT / "crates" / "conformance" / "src" / "runner.rs",
        2485,
    ),
    (
        "Emitter boundary: emitter/module_emission/core/mod.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "emitter"
        / "module_emission"
        / "core"
        / "mod.rs",
        2484,
    ),
    (
        "Emitter boundary: emitter/source_file/emit.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "emitter"
        / "source_file"
        / "emit.rs",
        2426,
    ),
    (
        "Checker boundary: jsdoc/diagnostics.rs size ratchet",
        ROOT / "crates" / "tsz-checker" / "src" / "jsdoc" / "diagnostics.rs",
        1450,
    ),
    (
        "Checker boundary: state/variable_checking/destructuring.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "state"
        / "variable_checking"
        / "destructuring.rs",
        1606,
    ),
    (
        "Checker boundary: state/state_checking_members/interface_checks.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "state"
        / "state_checking_members"
        / "interface_checks.rs",
        2250,
    ),
    (
        "Solver boundary: operations/constraints/walker.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-solver"
        / "src"
        / "operations"
        / "constraints"
        / "walker.rs",
        2230,
    ),
    # Ratcheted 2260→2263: +3 lines to thread catch_binding_ordinals
    # through the AsyncES5Emitter creation site.
    (
        "Emitter boundary: emitter/es5/helpers_async.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "emitter"
        / "es5"
        / "helpers_async.rs",
        2261,
    ),
    (
        "Checker boundary: state/variable_checking/core.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "state"
        / "variable_checking"
        / "core.rs",
        # Bumped 1979→1982 to record the live merged size after checker fixes
        # landed during a merge-queue (dist-binaries) outage window.
        1982,
    ),
    (
        "Emitter boundary: emitter/helpers.rs size ratchet",
        ROOT / "crates" / "tsz-emitter" / "src" / "emitter" / "helpers.rs",
        2222,
    ),
    (
        "Emitter boundary: declaration_emitter/usage_analyzer.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "declaration_emitter"
        / "usage_analyzer.rs",
        2154,
    ),
    (
        "Emitter boundary: emitter/transform_dispatch.rs size ratchet",
        ROOT / "crates" / "tsz-emitter" / "src" / "emitter" / "transform_dispatch.rs",
        2119,
    ),
    (
        "Solver boundary: visitors/visitor_predicates.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-solver"
        / "src"
        / "visitors"
        / "visitor_predicates.rs",
        2120,
    ),
    (
        "Solver boundary: operations/call_args.rs size ratchet",
        ROOT / "crates" / "tsz-solver" / "src" / "operations" / "call_args.rs",
        2097,
    ),
    (
        "LSP boundary: navigation/definition.rs size ratchet",
        ROOT / "crates" / "tsz-lsp" / "src" / "navigation" / "definition.rs",
        2121,
    ),
    (
        "Solver boundary: relations/subtype/rules/functions/checking.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-solver"
        / "src"
        / "relations"
        / "subtype"
        / "rules"
        / "functions"
        / "checking.rs",
        2198,
    ),
    (
        "LSP boundary: hierarchy/call_hierarchy.rs size ratchet",
        ROOT / "crates" / "tsz-lsp" / "src" / "hierarchy" / "call_hierarchy.rs",
        2091,
    ),
    (
        "Solver boundary: intern/core/interner.rs size ratchet",
        ROOT / "crates" / "tsz-solver" / "src" / "intern" / "core" / "interner.rs",
        2105,
    ),
    (
        "CLI boundary: bin/tsz_server/tests_navigation.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-cli"
        / "src"
        / "bin"
        / "tsz_server"
        / "tests_navigation.rs",
        2044,
    ),
    (
        "Solver boundary: operations/core/call_resolution.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-solver"
        / "src"
        / "operations"
        / "core"
        / "call_resolution.rs",
        1947,
    ),
    (
        "Solver boundary: caches/query_cache.rs size ratchet",
        ROOT / "crates" / "tsz-solver" / "src" / "caches" / "query_cache.rs",
        2022,
    ),
    (
        "LSP boundary: hover/core.rs size ratchet",
        ROOT / "crates" / "tsz-lsp" / "src" / "hover" / "core.rs",
        2029,
    ),
    (
        "Emitter boundary: emitter/statements/control_flow.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "emitter"
        / "statements"
        / "control_flow.rs",
        2014,
    ),
    (
        "Solver boundary: evaluation/evaluate.rs size ratchet",
        ROOT / "crates" / "tsz-solver" / "src" / "evaluation" / "evaluate.rs",
        2065,
    ),
    (
        "Emitter boundary: transforms/module_commonjs.rs size ratchet",
        ROOT / "crates" / "tsz-emitter" / "src" / "transforms" / "module_commonjs.rs",
        2016,
    ),
    # Current-main drift refresh (2026-06-12, #8204 ratchet PR): the
    # unguarded-oversized-file smoke test found these five production files
    # already past 2000 lines on main with no guard entry. Pinned at their
    # live counts; ratchet down as submodules land (§19).
    (
        "Solver boundary: def/core.rs size ratchet",
        ROOT / "crates" / "tsz-solver" / "src" / "def" / "core.rs",
        2298,
    ),
    (
        "Common boundary: perf_counters/runtime.rs size ratchet",
        ROOT / "crates" / "tsz-common" / "src" / "perf_counters" / "runtime.rs",
        2114,
    ),
    (
        "Solver boundary: evaluate_rules/index_access.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-solver"
        / "src"
        / "evaluation"
        / "evaluate_rules"
        / "index_access.rs",
        1835,
    ),
    (
        "Solver boundary: subtype/rules/generics.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-solver"
        / "src"
        / "relations"
        / "subtype"
        / "rules"
        / "generics.rs",
        2017,
    ),
    (
        "Solver boundary: intern/normalize.rs size ratchet",
        ROOT / "crates" / "tsz-solver" / "src" / "intern" / "normalize.rs",
        2010,
    ),
    # Repo-wide coverage refresh (2026-08-07, #16733): when the directory-level
    # 2000-LOC cap was extended from tsz-checker/src to every crates/*/src root,
    # the unguarded-oversized-file smoke test surfaced these production files
    # already past 2000 lines on main with no per-file ratchet. Pinned at their
    # live counts so they cannot grow further; ratchet down as submodules land
    # (§19).
    (
        "Parser boundary: parser/state_expressions_literals_regex.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-parser"
        / "src"
        / "parser"
        / "state_expressions_literals_regex.rs",
        2438,
    ),
    (
        "CLI boundary: driver/check_utils/tests.rs size ratchet",
        ROOT / "crates" / "tsz-cli" / "src" / "driver" / "check_utils" / "tests.rs",
        2258,
    ),
    (
        "Solver boundary: relations/subtype/rules/objects.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-solver"
        / "src"
        / "relations"
        / "subtype"
        / "rules"
        / "objects.rs",
        2075,
    ),
    (
        "Lowering boundary: lower/core.rs size ratchet",
        ROOT / "crates" / "tsz-lowering" / "src" / "lower" / "core.rs",
        2032,
    ),
    (
        "Binder boundary: state/core.rs size ratchet",
        ROOT / "crates" / "tsz-binder" / "src" / "state" / "core.rs",
        2006,
    ),
    (
        "Solver boundary: contextual/extractors.rs size ratchet",
        ROOT / "crates" / "tsz-solver" / "src" / "contextual" / "extractors.rs",
        2004,
    ),
    # parser/state_declarations.rs dropped below the 2000-line cap once enum
    # declaration parsing moved to state_declarations_enums.rs; its ratchet
    # entry was removed (the allowlist only shrinks).
]
