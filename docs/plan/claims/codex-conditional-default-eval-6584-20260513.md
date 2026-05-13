# fix(solver): evaluate conditional generic defaults

- **Date**: 2026-05-13
- **Branch**: `codex/conditional-default-eval-6584-20260513`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance / solver false positives

## Intent

Fix #6584 so defaulted type parameters whose default is a conditional type
depending on earlier known parameters are instantiated and evaluated. The
target regression is `Wrapper<string>` where `W = T extends string ? number :
boolean` must resolve to `number`.

## Files Touched

- `crates/tsz-solver/src/*` (expected)
- `crates/tsz-checker/tests/generic_call_inference_tests.rs` (expected)
- `docs/plan/claims/codex-conditional-default-eval-6584-20260513.md`

## Verification

- Pending.
