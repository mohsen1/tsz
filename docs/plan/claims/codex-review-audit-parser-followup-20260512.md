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

## Verification

- `cargo fmt --check`
- `cargo test -p tsz-parser`
- `python3 scripts/session/audit_missed_review_comments.py --limit 500` (latest successful run: `candidate_count=137`)
