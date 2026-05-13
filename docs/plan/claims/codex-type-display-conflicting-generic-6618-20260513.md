# fix(checker): avoid type display panic for conflicting generic calls

- **Date**: 2026-05-13
- **Branch**: `codex/type-display-conflicting-generic-6618-20260513`
- **PR**: #6619
- **Status**: ready
- **Workstream**: checker / crash

## Intent

Fix #6618 so TS2345 reporting for a generic call with conflicting argument
types does not panic while normalizing type display text.

## Files Touched

- `crates/tsz-checker/src/error_reporter/core/type_display.rs`
- `crates/tsz-checker/Cargo.toml`
- `crates/tsz-checker/tests/generic_conflicting_argument_type_display_tests.rs`
- `docs/plan/claims/codex-type-display-conflicting-generic-6618-20260513.md`

## Verification

- `cargo test -p tsz-checker --test generic_conflicting_argument_type_display_tests conflicting_generic_argument_reports_ts2345_without_type_display_panic -- --nocapture` (1 passed)
- `cargo fmt --all --check`
