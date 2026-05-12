# fix(audit): follow up missed review items for diagnostics and parser guardrails

- **Date**: 2026-05-12
- **Branch**: `codex/review-audit-followup-20260512`
- **PR**: #5820
- **Status**: ready
- **Workstream**: review audit follow-ups

## Intent

This PR lands focused follow-ups from missed high-signal review comments in the 500-PR audit pass. It tightens diagnostic display behavior, parser guardrail coverage, and brittle regression assertions without broad refactors.

## Files Touched

- `crates/tsz-solver/src/diagnostics/format/compound.rs` (avoid over-eliding non-recursive `typeof` function returns)
- `crates/tsz-checker/src/error_reporter/core_formatting.rs` (preserve enum-member union source display)
- `crates/tsz-parser/src/parser/node_arena.rs` (lock `NodeArena::len_u32` overflow panic message)
- `crates/tsz-checker/tests/generic_call_inference_tests.rs` (select TS2345 anchor by argument span)
- `crates/tsz-checker/tests/conditional_infer_tests.rs` (reject unexpected extra diagnostics)
- `crates/tsz-checker/tests/ts2315_explicit_any_type_alias_tests.rs` (guard against TS2344 cascades)
- `crates/tsz-checker/tests/recursive_typeof_param_display_tests.rs` (non-recursive `typeof` display coverage)
- `crates/tsz-checker/tests/ts2322_tests.rs` (enum-member union display coverage)

## Verification

- `cargo fmt --all`
- `cargo test -p tsz-checker --test recursive_typeof_param_display_tests -- --nocapture`
- `cargo test -p tsz-checker --test ts2322_tests test_ts2322_same_enum_member_union_source_display_preserved -- --nocapture`
- `cargo test -p tsz-checker --test ts2322_tests enum -- --nocapture`
- `cargo test -p tsz-parser len_u32_overflow_panics_with_expected_message -- --nocapture`
- `cargo test -p tsz-checker --test generic_call_inference_tests self_referential_constraint_fallback_anchors_first_argument_after_contextual_assignment -- --nocapture`
- `cargo test -p tsz-checker --test conditional_infer_tests conditional_keyof_variance_assignability_matches_tsc -- --nocapture`
- `cargo test -p tsz-checker --test ts2315_explicit_any_type_alias_tests ts2315_fires_on_parenthesized_explicit_any_alias_body -- --nocapture`
