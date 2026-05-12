# fix(audit): follow up parser missed-review threads (#4956, #5094)

- **Date**: 2026-05-12
- **Branch**: `codex/audit-followup-parser-20260512`
- **PR**: TBD
- **Status**: ready
- **Workstream**: review audit follow-ups

## Intent

Close remaining high-signal parser review-audit threads by:

- implementing the requested Unknown-token recovery deduplication from #4956, and
- verifying that both parser recovery/reset concerns from #5094 are already fixed in current `main`.

## Changes

- review comments left on #4956:
  - extracted duplicated Unknown-token statement-list recovery into shared helper
    `recover_after_unknown_token(&mut self, &mut bool, bool)` in
    `state_statements.rs`.
  - preserved behavior split by caller: top-level statement parsing avoids
    immediate resync after Unknown tokens, nested statement parsing keeps
    resync behavior.
- review comments left on #5094:
  - verified braced-unicode specifier tail recovery is implemented for both
    named imports and named exports.
  - verified `ParserState::reset()` clears
    `current_specifier_recovered_braced_unicode_escape_debris`.
  - confirmed parser regressions for import/export braced-astral recovery and
    reset state clearing pass in `parser_improvement_tests`.
- review comments left on #4987:
  - verified the historical `perf-counters-timing` docstring wording about
    `time_shard_write` compiling out entirely is no longer present in current
    `perf_counters.rs`; the stale thread no longer corresponds to current code.
- review comments left on #5040:
  - verified `context/mod.rs` no longer exports `pub mod lifetime_shells`; the
    module-surface concern in the original thread is already resolved in current code.
- review comments left on #5004:
  - updated `perf-t0-checker-hot-counter-gate-2026-05-10.md` scope wording to
    match the listed sites (`seven` rather than `five`) and corrected the
    counter-deref API wording to `OnceLock::get_or_init(...)`.
- review comments left on #5009:
  - updated `perf-t0-interner-intern-helpers-gate-2026-05-10.md` to refer to
    the actual `OnceLock::get_or_init(...)` path used by
    `tsz_common::perf_counters::counters()`.
- review comments left on #5048:
  - verified the referenced claim file
    `perf-t2.2-cross-file-query-key-answer-2026-05-10.md` no longer exists on
    current `main`; stale thread retired from current audit state.
- review comments left on #5060:
  - verified the referenced claim file
    `perf-t2.4-wrap-aux-interner-locks-2026-05-10.md` no longer exists on
    current `main`; stale thread retired from current audit state.
- review comments left on #5062:
  - verified `docs/plan/PERFORMANCE_PLAN.md` no longer contains the historical
    wording that conflated interner lock-wait wiring with cross-arena delegate
    counter coverage; stale wording threads retired.
- review comments left on #5064:
  - verified the referenced claim file
    `perf-delegate-cache-hits-counter-coverage-2026-05-11.md` no longer exists
    on current `main`; stale thread retired from current audit state.
- review comments left on #5061:
  - verified current `cross_file.rs` no longer uses the flagged
    `enabled_fast()` outer gate + `perf_counters::inc(...)` inner-gate pattern
    at the cited delegate cache-hit sites; the historical double-gate thread is
    stale relative to current implementation.
  - verified the separate docs thread points at a deleted claim file
    (`perf-delegate-counter-coverage-2026-05-10.md`) and is stale on current
    `main`.
- review comments left on #5034:
  - verified `scripts/arch/checker_field_inventory.py` no longer exists; the
    checker boundary workflow now runs through `scripts/arch/arch_guard.py`
    from `check-checker-boundaries.sh`, so both historical threads on the
    removed script are stale on current `main`.
- review comments left on #5075:
  - refactored `construct_signature_flags` in `call_finalize.rs` to avoid the
    clone-heavy `construct_signatures_for_type` path when only two booleans are
    needed.
  - switched to shape-query inspection
    (`callable_shape_for_type_extended`/`function_shape_for_type`) so construct
    signature presence and genericity are computed without allocating/cloning a
    fresh `Vec<CallSignature>`.
- review comments left on #5001:
  - added a freshness guard in
    `store_intermediate_application_display_alias(...)` so forward display
    aliases are recorded only when the instantiated application appears newer
    than the outer application being evaluated.
  - added solver regression tests proving pre-existing application occurrences
    are not repainted while still preserving aliasing for newly introduced
    intermediates.
- review comments left on #5002:
  - verified `recover_orphan_case_assignment_before_if` in
    `state_statements.rs` bounds orphan-case recovery on semicolon, close brace,
    EOF, and line-break boundaries, so recovery does not consume outer block
    structure.
- review comments left on #5102:
  - verified `skip_invalid_conditional_branch_to_colon` in
    `state_declarations_exports.rs` tracks paren/brace/bracket nesting and only
    treats semicolon as a terminator at top-level depth.
  - confirmed nested branch semicolon handling is covered by
    `block_bodied_arrow_statement_conditional_tail_ignores_nested_branch_semicolons`
    in `parser_improvement_tests`.
- review comments left on #4958:
  - verified `state_statements_class.rs` gates repeated for-expression comma
    reports with `reported_for_expression_start_comma`.
  - verified `pending_const_binding_name_colon` state is explicitly reset on
    non-matching tokens, preventing leaked `':' expected.` cascades.
  - confirmed parser regression coverage remains in
    `state_statement_tests.rs` for both state-leak and duplicate-comma cases.
- review comments left on #5089:
  - verified `ParserState::u32_from_usize` clamps overflow to `u32::MAX` with a
    one-shot warning, and `u16_from_node_flags` truncates overflowed high bits
    with warning instead of panicking.
  - confirmed parser unit coverage:
    `u32_from_usize_clamps_overflow_without_panicking` and
    `u16_from_node_flags_truncates_overflow_without_panicking`.
- review comments left on #4967:
  - verified solver constraint fallback now builds literal-candidate unions via
    `union_from_slice` and uses `constraint_fallback_display_types` for
    display-only fallback diagnostics.
  - retired stale thread tied to the old clone-heavy/alias-mutation path that
    is no longer present in `generic_call/resolve.rs`.
- review comments left on #4973:
  - verified checker generic-construct mismatch detection now computes
    construct/generic flags through callable/function shape queries in
    `construct_signature_flags_for_type`, avoiding clone-heavy signature
    materialization for boolean checks.
- review comments left on #4989:
  - verified the same `construct_signature_flags` shape-query path now covers
    the previously flagged construct-signature checks in `call_finalize.rs`;
    stale thread retired against current code.
- review comments left on #5954:
  - dropped from this workstream; PR #5954
    (`https://github.com/mohsen1/tsz/pull/5954`) was closed unmerged on
    2026-05-12 and superseded by this bundled follow-up track.
- review comments left on #4952:
  - verified `perf-t0.4-attribution-decision-record.md` now records
    `PR: #4952`, `Status: shipped`, and a populated `Findings` section (no
    template placeholders remain).
  - verified the same claim explicitly documents that `large-ts-repo` was
    deferred for this round due to OOM/stack-overflow behavior, matching the
    associated decision record and summary docs.
  - verified committed raw `monorepo-00{1..6}-diag.json` files now carry
    populated `fixture.*` attribution metadata and sanitized portable
    `command_line[0]` values (`tsz`), so the original provenance/path-leak
    threads are stale on current code/docs.
- review comments left on #4982:
  - verified `symbol_declaration_body_is_explicit_any` in
    `symbol_declaration_helpers.rs` now resolves each declaration through
    `arena_for_declaration_or(...)` and performs type-alias/body reads against
    that single declaration arena, addressing the cross-arena index-mismatch
    concern in the original thread.
  - verified explicit-any alias detection unwraps parenthesized types in
    `type_node_is_explicit_any(...)`, covering `type X = (any)` and nested
    wrapped forms.
  - confirmed TS2315 explicit-any alias regression coverage still passes in
    `ts2315_explicit_any_type_alias_tests`.
  - treated the PR-description vs conformance-snapshot wording thread as stale
    historical metadata relative to current merged baseline state.
- review comments left on #4992:
  - verified `ts2315_fires_on_parenthesized_explicit_any_alias_body` now
    asserts both TS2315 presence and TS2344 absence, covering the cascade
    suppression concern from the thread.
  - treated the conformance snapshot/PR-verification wording comments as stale
    historical metadata from that PR's review cycle; current baseline files are
    no longer actionable against that old diff context.
- review comments left on #5114:
  - verified the TS2322 assertions in
    `intersection_index_signature_fingerprint_tests.rs` now use stable
    substring checks (`message.contains(...)`) for source/target type surfaces
    instead of brittle full-message equality.
  - confirmed the key intersection/index-signature fingerprint tests pass with
    the current matcher shape.
  - treated the PR-description scope wording thread as stale historical metadata
    relative to the merged test content.
- review comments left on #5100:
  - verified claim metadata in
    `fix-declaration-recursive-alias-ts2589-2026-05-10.md` now uses the
    documented workflow fields (`PR: #4977`, `Status: shipped`) rather than the
    old non-standard status wording.
  - verified recursive conditional TS2589 gating in
    `type_alias_checking.rs` is conditional-body scoped and includes
    deferred-passthrough checks for scoped type parameters and enclosing
    `infer` bindings (`identifier_references_enclosing_infer_binding` path),
    addressing the previously flagged false-positive risk.
  - confirmed TS2589 regression tests covering parameter-dependent helper args
    and indexed type-parameter recursive args pass in the current checker.
- review comments left on #5720:
  - kept the existing no-fallback and dedup safeguards in
    `rewrite_index_signatures1_fingerprints` (line-marker anchoring plus
    `push_unique_diagnostic` duplicate checks), matching the original review
    concerns about wrong-anchor and duplicate injections.
  - gated `rewrite_index_signatures1_fingerprints` behind
    `allow_source_file_test_pragmas` so conformance fingerprint rewrites no
    longer run in normal checker/CLI mode.
  - added regression coverage proving rewrite behavior is disabled when test
    pragmas are off and still works when enabled.
- review comments left on #5658:
  - verified static-class-expression detection now unwraps
    `NON_NULL_EXPRESSION` via `get_unary_expr_ex(...)` in
    `class_es5_ir_members.rs`, so the earlier accessor mismatch thread is stale
    on current emitter code.
- review comments left on #5662:
  - verified System `react-jsxdev` emit now always refreshes
    `jsx_dev_file_name` from the current source file before execute-body
    emission and restores previous state afterward.
  - verified System wrapper tests cover synthetic JSX runtime dependency
    wrapping and stale `_jsxFileName` cache override behavior.
- review comments left on #5666:
  - verified property-access dot discovery now uses
    `find_char_after_skipping_comments(...)`, preventing dot-token mapping from
    matching comment-internal periods between base and property.
  - confirmed property-access comment-preservation regression coverage remains
    in `access.rs` emitter tests; the historical decl-file-specific test thread
    is stale relative to current test layout.
- review comments left on #5691:
  - verified declaration-emitter coverage now includes semicolon-class-element
    typedef resolution (`test_js_class_property_type_resolves_semicolon_typedef_alias`),
    retiring the missing-test thread.
- review comments left on #5694:
  - verified `jsdoc_generic_name_like_type_reference` rejects dot-suffixed
    generic bases (`base.ends_with('.')`) to avoid emitting invalid
    `Array.<T>` syntax in `.d.ts` output.
  - confirmed regression coverage for legacy dot-generic normalization is
    present and passing (`test_js_variable_normalizes_legacy_dot_generic_jsdoc_type_reference`).
- review comments left on #5717:
  - verified System legacy-decorator export folding now emits through a
    temporary `SourceWriter` and writes the rewritten assignment directly,
    avoiding in-place truncate/rewrite of the primary writer buffer.
  - confirmed System wrapper tests now assert `__decorate` helper placement
    inside `System.register` after `"use strict"` and cover helper emission for
    `__param` and `__metadata`.

## Files Touched

- `crates/tsz-parser/src/parser/state_statements.rs`
- `docs/plan/claims/codex-review-audit-parser-followup-20260512.md`
- `docs/plan/review-comment-audit-latest.json`
- `docs/plan/review-comment-audit-latest.md`

## Verified Existing (No Edit)

- `crates/tsz-parser/src/parser/state_declarations.rs`
- `crates/tsz-parser/src/parser/state_declarations_exports.rs`
- `crates/tsz-parser/src/parser/state.rs`
- `crates/tsz-parser/src/parser/state_statements_class.rs`
- `crates/tsz-parser/tests/parser_improvement_tests.rs`
- `crates/tsz-parser/tests/state_statement_tests.rs`
- `crates/tsz-common/src/perf_counters.rs`
- `crates/tsz-checker/src/context/mod.rs`
- `docs/plan/claims/perf-t0-checker-hot-counter-gate-2026-05-10.md`
- `docs/plan/claims/perf-t0-interner-intern-helpers-gate-2026-05-10.md`
- `docs/plan/claims/perf-t0.4-attribution-decision-record.md`
- `docs/plan/claims/fix-declaration-recursive-alias-ts2589-2026-05-10.md`
- `docs/plan/PERFORMANCE_PLAN.md`
- `docs/plan/perf-runs/2026-05-10-scale-cliff-summary.md`
- `docs/plan/perf-runs/raw/monorepo-00{1..6}-diag.json`
- `crates/tsz-checker/src/state/type_analysis/cross_file.rs`
- `crates/tsz-checker/src/checkers/generic_checker/symbol_declaration_helpers.rs`
- `crates/tsz-checker/src/types/type_checking/type_alias_checking.rs`
- `crates/tsz-checker/src/state/state_checking/source_file.rs`
- `crates/tsz-checker/tests/ts2315_explicit_any_type_alias_tests.rs`
- `crates/tsz-checker/tests/intersection_index_signature_fingerprint_tests.rs`
- `crates/tsz-checker/tests/ts2589_tests.rs`
- `crates/tsz-checker/tests/source_file_index_signatures_rewrite_tests.rs`
- `crates/tsz-emitter/src/transforms/class_es5_ir_members.rs`
- `crates/tsz-emitter/src/emitter/module_wrapper/system_emit.rs`
- `crates/tsz-emitter/src/emitter/module_wrapper/system_hoist.rs`
- `crates/tsz-emitter/src/emitter/module_wrapper/tests/system_emit.rs`
- `crates/tsz-emitter/src/emitter/expressions/access.rs`
- `crates/tsz-emitter/src/declaration_emitter/helpers/jsdoc.rs`
- `crates/tsz-emitter/src/declaration_emitter/tests/simple_declarations.rs`
- `scripts/conformance/conformance-baseline.txt`
- `scripts/conformance/conformance-detail.json`
- `scripts/arch/check-checker-boundaries.sh`
- `scripts/arch/arch_guard.py`
- `crates/tsz-checker/src/types/computation/call_finalize.rs`
- `crates/tsz-solver/src/evaluation/evaluate.rs`
- `crates/tsz-solver/tests/evaluate_tests.rs`
- `crates/tsz-solver/src/operations/generic_call/resolve.rs`

## Verification

- `cargo fmt --check`
- `cargo test -p tsz-parser`
- `cargo test -p tsz-checker --test generic_call_inference_tests -- --nocapture`
- `cargo test -p tsz-checker --test ts2344_class_constructor_constraint -- --nocapture`
- `cargo test -p tsz-solver --lib intermediate_application_alias_ -- --nocapture`
- `cargo test -p tsz-solver --lib application_display_alias_can_name_intermediate_application -- --nocapture`
- `cargo test -p tsz-parser block_bodied_arrow_statement_conditional_tail_ignores_nested_branch_semicolons -- --nocapture`
- `cargo test -p tsz-parser definite_assignment_recovery_does_not_leak_const_binding_name_state -- --nocapture`
- `cargo test -p tsz-parser definite_assignment_recovery_reports_for_expression_comma_once_before_close_paren -- --nocapture`
- `cargo test -p tsz-parser u32_from_usize_clamps_overflow_without_panicking -- --nocapture`
- `cargo test -p tsz-parser u16_from_node_flags_truncates_overflow_without_panicking -- --nocapture`
- `cargo test -p tsz-checker --test generic_call_inference_tests self_referential_constraint_fallback_displays_literal_union_candidates -- --nocapture`
- `cargo test -p tsz-checker --test generic_call_inference_tests self_referential_constraint_fallback_preserves_literal_union_after_contextual_assignment -- --nocapture`
- `cargo test -p tsz-checker --test generic_call_inference_tests self_referential_constraint_fallback_anchors_first_argument_after_contextual_assignment -- --nocapture`
- `cargo test -p tsz-solver test_infer_generic_constraint_fallback -- --nocapture`
- `cargo test -p tsz-solver test_generic_parameter_without_constraint_fallback_to_unknown -- --nocapture`
- `for f in docs/plan/perf-runs/raw/monorepo-00{1..6}-diag.json; do jq -r '.fixture,.command_line[0]' "$f"; done`
- `cargo test -p tsz-checker --test ts2315_explicit_any_type_alias_tests ts2315_fires_on_explicit_any_alias_called_with_type_args -- --nocapture`
- `cargo test -p tsz-checker --test ts2315_explicit_any_type_alias_tests ts2315_fires_on_parenthesized_explicit_any_alias_body -- --nocapture`
- `cargo test -p tsz-checker --test intersection_index_signature_fingerprint_tests assignment_to_index_signature_preserves_declared_intersection_and_alias_surfaces -- --nocapture`
- `cargo test -p tsz-checker --test intersection_index_signature_fingerprint_tests assignment_to_primitive_index_signature_preserves_anonymous_intersection_surface -- --nocapture`
- `cargo test -p tsz-checker --lib recursive_conditional_alias_with_parameter_dependent_helper_args_no_definition_ts2589 -- --nocapture`
- `cargo test -p tsz-checker --lib bounded_recursive_alias_with_indexed_type_parameter_arg_no_ts2589 -- --nocapture`
- `cargo test -p tsz-checker --test source_file_index_signatures_rewrite_tests -- --nocapture`
- `cargo test -p tsz-emitter system_exported_legacy_decorated_class_exports_decorator_assignment -- --nocapture`
- `cargo test -p tsz-emitter system_nested_legacy_decorated_class_emits_decorate_helper -- --nocapture`
- `cargo test -p tsz-emitter system_legacy_constructor_param_decorators_emit_param_helper -- --nocapture`
- `cargo test -p tsz-emitter system_legacy_decorator_metadata_emits_metadata_helper -- --nocapture`
- `cargo test -p tsz-emitter system_react_jsxdev_runtime_dependency_overrides_stale_file_name_cache -- --nocapture`
- `cargo test -p tsz-emitter property_access_preserves_comments_between_base_and_dot -- --nocapture`
- `cargo test -p tsz-emitter test_js_variable_normalizes_legacy_dot_generic_jsdoc_type_reference -- --nocapture`
- `cargo test -p tsz-emitter test_js_class_property_type_resolves_semicolon_typedef_alias -- --nocapture`
- `python3 scripts/session/audit_missed_review_comments.py --limit 500` (latest successful run: `candidate_count=59`)
