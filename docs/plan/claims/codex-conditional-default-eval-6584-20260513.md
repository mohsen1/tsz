# fix(solver): evaluate conditional generic defaults

- **Date**: 2026-05-13
- **Branch**: `codex/conditional-default-eval-6584-20260513`
- **PR**: #6617
- **Status**: ready
- **Workstream**: conformance / solver false positives

## Intent

Cover #6584 so defaulted type parameters whose default is a conditional type
depending on earlier known parameters remain instantiated and evaluated. The
target regression is `Wrapper<string>` where `W = T extends string ? number :
boolean` must resolve to `number`.

Current `main` already passes the regression after preceding inference fixes, so
this PR adds coverage without solver changes.

## Files Touched

- `crates/tsz-checker/tests/generic_call_inference_tests.rs`
- `docs/plan/claims/codex-conditional-default-eval-6584-20260513.md`

## Verification

- `cargo test -p tsz-checker --test generic_call_inference_tests conditional_type_parameter_default_evaluates_after_prior_arg_known -- --nocapture` (1 passed)
- `cargo test -p tsz-checker --test generic_call_inference_tests -- --nocapture` (155 passed)
- `cargo fmt --all --check`
