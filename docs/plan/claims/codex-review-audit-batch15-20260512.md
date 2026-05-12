# perf(audit): retire recursive typeof display follow-ups

- **Date**: 2026-05-12
- **Branch**: `codex/review-audit-batch14-20260512`
- **PR**: TBD
- **Status**: ready
- **Workstream**: review audit follow-ups

## Intent

Close review comments left on #5663 by confirming the recursive/non-recursive
`typeof` display behavior is covered and by removing an unnecessary eager
formatting call in call-parameter diagnostic rendering.

## Changes

- review comments left on #5663:
  - confirmed recursive `typeof` return elision parity remains covered by:
    - `recursive_typeof_function_target_display_elides_nested_return_cycle`
    - `non_recursive_nested_typeof_function_return_does_not_elide`
  - moved `direct_param_display = format_type_diagnostic(param_type)` below
    the recursive-`typeof`/tuple/rest fast paths in
    `format_call_parameter_type_for_diagnostic`, so we skip that formatting work
    when an earlier branch already returns.

## Files Touched

- `crates/tsz-checker/src/error_reporter/call_errors/display_formatting.rs`
- `docs/plan/claims/codex-review-audit-batch15-20260512.md`
- `docs/plan/review-comment-audit-latest.json`
- `docs/plan/review-comment-audit-latest.md`

## Verification

- `cargo test -p tsz-checker --test recursive_typeof_param_display_tests -- --nocapture`
- `python3 scripts/session/audit_missed_review_comments.py --limit 500`
