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

## Files Touched

- `crates/tsz-parser/src/parser/state_statements.rs`
- `docs/plan/claims/codex-review-audit-parser-followup-20260512.md`
- `docs/plan/review-comment-audit-latest.json`
- `docs/plan/review-comment-audit-latest.md`

## Verified Existing (No Edit)

- `crates/tsz-parser/src/parser/state_declarations.rs`
- `crates/tsz-parser/src/parser/state_declarations_exports.rs`
- `crates/tsz-parser/src/parser/state.rs`
- `crates/tsz-parser/tests/parser_improvement_tests.rs`
- `crates/tsz-common/src/perf_counters.rs`
- `crates/tsz-checker/src/context/mod.rs`
- `docs/plan/claims/perf-t0-checker-hot-counter-gate-2026-05-10.md`
- `docs/plan/claims/perf-t0-interner-intern-helpers-gate-2026-05-10.md`
- `docs/plan/PERFORMANCE_PLAN.md`
- `crates/tsz-checker/src/state/type_analysis/cross_file.rs`
- `scripts/arch/check-checker-boundaries.sh`
- `scripts/arch/arch_guard.py`
- `crates/tsz-checker/src/types/computation/call_finalize.rs`

## Verification

- `cargo fmt --check`
- `cargo test -p tsz-parser`
- `cargo test -p tsz-checker --test generic_call_inference_tests -- --nocapture`
- `cargo test -p tsz-checker --test ts2344_class_constructor_constraint -- --nocapture`
- `python3 scripts/session/audit_missed_review_comments.py --limit 500` (latest successful run: `candidate_count=124`)
