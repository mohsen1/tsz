# chore(checker): tidy implicit-any option checks

- **Date**: 2026-05-11
- **Branch**: `codex/cleanup-implicit-any-option-20260512`
- **PR**: #5651
- **Status**: ready
- **Workstream**: DRY cleanup

## Intent

Replace the local `!option.is_none()` patterns in implicit-any member checks
with the idiomatic `option.is_some()` form. This is a behavior-preserving cleanup
slice from the repo's code-cleanup prompt and intentionally stays limited to one
checker file.

## Files Touched

- `crates/tsz-checker/src/state/state_checking_members/implicit_any_checks.rs`
- `docs/plan/claims/codex-cleanup-implicit-any-option-20260512.md`

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker implicit_any` (117 passed, 7390 skipped)
