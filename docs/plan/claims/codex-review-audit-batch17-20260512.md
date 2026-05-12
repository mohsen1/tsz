# fix(audit): preserve contextual initializer cache through jsdoc raw-new relation check

- **Date**: 2026-05-12
- **Branch**: `codex/review-audit-batch17-20260512`
- **PR**: TBD
- **Status**: ready
- **Workstream**: review audit follow-ups

## Intent

Close the important unresolved review comment on #5690 about cache integrity in
`check_variable_declaration_with_request` when the JSDoc `@type` + `new`
initializer relation path performs an additional raw initializer re-check.

## Changes

- review comments left on #5690:
  - snapshot and restore the initializer entry in `ctx.node_types` around the
    `TypingRequest::NONE` re-check inside `jsdoc_new_expression_relation`.
  - keep the raw relation check behavior for assignability parity, while
    preventing the raw pass from permanently overwriting the context-seeded
    initializer cache entry.
- review comments left on #4967:
  - verified current `generic_call/resolve.rs` no longer stores a global
    `display_alias` on fallback constraints; it now records fallback display
    types in call-scoped `constraint_fallback_display_types`.
  - verified the previously flagged clone-heavy patterns are no longer present:
    no `display_subst = final_subst.clone()` display override path and no
    `un_widened.clone()` union construction in this fallback branch.
- review comments left on #4982:
  - verified `symbol_declaration_body_is_explicit_any` now resolves declaration
    ownership with `binder.arena_for_declaration_or(sym_id, decl_idx, ...)`
    and performs alias/body lookups against that selected arena (no cross-arena
    `NodeIndex` probing against mismatched arenas).
  - verified explicit-`any` detection now unwraps parenthesized type nodes via
    `type_node_is_explicit_any`, covering `type X = (any)` and nested wrappers.
  - the historical PR-description/conformance-baseline mismatch thread is stale
    relative to current baseline churn; no code-side follow-up remains.
- review comments left on #5100:
  - verified claim metadata now uses allowed status token (`shipped`) in
    `docs/plan/claims/fix-declaration-recursive-alias-ts2589-2026-05-10.md`.
  - verified recursive-alias depth checks are now conditional-body scoped
    (`body_is_conditional` gates) and deferred-passthrough aware
    (`type_node_is_deferred_passthrough_for_depth_check`) in
    `type_alias_checking.rs`.
  - reran `ts2589_tests` suite to confirm the intended definition-site vs
    instantiation-site TS2589 behavior remains covered.
- review comments left on #5899:
  - updated emitter literal tests helper `parse_test_source` to accept
    `Into<String>` instead of `&str + to_string()`, removing an avoidable clone
    when tests already own a `String` fixture.
  - updated the joined-source test call site to pass owned `String` directly.
- review comments left on #4977:
  - verified `conditional_body_has_unresolved_computed_recursive_alias_ref(...)`
    is now guarded by `body_is_conditional` in `type_alias_checking.rs`, so
    non-conditional aliases no longer pay/trigger that path.
- review comments left on #4951:
  - replaced O(n²) literal candidate de-duplication in
    `generic_application_literal_expected_for_mismatch` with `FxHashSet`-based
    seen tracking while preserving output order in `candidates`.
  - behavior remains unchanged; this is a hot-path complexity reduction only.
- review comments left on #4991:
  - strengthened wasm regression assertion to require zero semantic diagnostics
    for the nested anonymous object-literal assignment case, instead of only
    checking TS2322 absence.
- review comments left on #4949:
  - verified `find_directive_in_text` still returns and consumes directive byte
    offsets through `find_ts_directives` (`directive_start -> directive_line`)
    for TS2578 anchoring behavior.
  - reran directive-anchor regression tests for indented and multiline comment
    forms in `tsz-cli` (`unused_expect_error_*`).
  - historical PR-description/snapshot delta thread is stale relative to later
    snapshot churn; no code-side follow-up remains.

## Files Touched

- `crates/tsz-checker/src/state/variable_checking/core.rs`
- `crates/tsz-solver/src/operations/generic_call/resolve.rs` (verified current behavior; no edit needed)
- `crates/tsz-checker/src/checkers/generic_checker/symbol_declaration_helpers.rs` (verified current behavior; no edit needed)
- `crates/tsz-checker/src/types/type_checking/type_alias_checking.rs` (verified current behavior; no edit needed)
- `docs/plan/claims/fix-declaration-recursive-alias-ts2589-2026-05-10.md` (verified current status token)
- `crates/tsz-emitter/src/emitter/literals/core.rs`
- `docs/plan/claims/codex-review-audit-batch17-20260512.md`
- `crates/tsz-checker/src/types/computation/call_result.rs`
- `crates/tsz-wasm/src/wasm_tests.rs`
- `crates/tsz-cli/src/driver/check_utils.rs` (verified current behavior; no edit needed)
- `docs/plan/review-comment-audit-latest.json`
- `docs/plan/review-comment-audit-latest.md`

## Verification

- `cargo test -p tsz-checker --test jsdoc_cross_file_typedef_tests jsdoc_type_assignment_new_expression_reports_subclass_mismatch -- --nocapture`
- `cargo test -p tsz-checker --test jsdoc_cross_file_typedef_tests jsdoc_type_assignment_binds_interface_this_to_source_instance -- --nocapture`
- `cargo test -p tsz-checker --test generic_call_primitive_widening_display_tests -- --nocapture`
- `cargo test -p tsz-checker --test ts2315_explicit_any_type_alias_tests -- --nocapture`
- `cargo test -p tsz-checker --lib ts2589_tests -- --nocapture`
- `cargo test -p tsz-emitter regex_literal_preserves_non_ascii_flags -- --nocapture`
- `cargo test -p tsz-emitter decimal_numeric_separators_with_exponents_downlevel_to_number_text -- --nocapture`
- `cargo test -p tsz-wasm ts_program_accepts_nested_anonymous_object_literal_assignment -- --nocapture`
- `cargo test -p tsz-cli unused_expect_error_ -- --nocapture`
- `cargo fmt --all --check`
- `python3 scripts/session/audit_missed_review_comments.py --limit 500` (last successful run: `candidate_count=56`; subsequent attempts blocked by GitHub GraphQL rate limit)
