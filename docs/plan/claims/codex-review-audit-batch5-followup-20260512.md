# fix(checker): follow up recursive-alias review

- **Date**: 2026-05-12
- **Branch**: `codex/review-audit-batch5-followup-20260512`
- **PR**: TBD
- **Status**: ready
- **Workstream**: review audit follow-ups

## Intent

Land the review follow-up from #5889 after that PR merged before the review
fix reached its head branch. The checker now avoids the definite recursive-ref
scan for non-conditional alias bodies, matching the documented TS2589
definition-time gate, and the original claim heading is aligned with #5889's
PR title.

## Files Touched

- `crates/tsz-checker/src/types/type_checking/type_alias_checking.rs`
- `docs/plan/claims/codex-review-audit-batch5-20260512.md`
- `docs/plan/claims/codex-review-audit-batch5-followup-20260512.md`

## Verification

- `cargo test -p tsz-checker --lib ts2589_tests::non_conditional_recursive_alias_with_unresolved_qualified_arg_no_ts2589 -- --exact --nocapture`
- `cargo test -p tsz-checker --lib ts2589_tests::recursive_conditional_type_alias_definition_emits_ts2589 -- --exact --nocapture`
