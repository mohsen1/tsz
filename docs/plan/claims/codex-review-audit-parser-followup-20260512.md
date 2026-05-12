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
- `docs/plan/PERFORMANCE_PLAN.md`
- `crates/tsz-checker/src/state/type_analysis/cross_file.rs`
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
- `python3 scripts/session/audit_missed_review_comments.py --limit 500` (latest successful run: `candidate_count=124`)
- `python3 scripts/session/audit_missed_review_comments.py --limit 500` is currently blocked by GitHub GraphQL rate-limit exhaustion until reset at `2026-05-12T21:09:52Z`.
