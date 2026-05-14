# fix(checker): reject string for generic array overload

- **Date**: 2026-05-13
- **Branch**: `codex/overload-array-string-6624-20260513`
- **PR**: #6643
- **Status**: ready
- **Workstream**: conformance / overload resolution

## Intent

Fix #6624 so overload resolution does not accept a string argument for a
generic `T[]` parameter before trying a later `string` overload.

## Files Touched

- `crates/tsz-checker/src/checkers/call_checker/overload_resolution.rs`
- `crates/tsz-checker/src/tests/dispatch_tests.rs`
- `docs/plan/claims/codex-overload-array-string-6624-20260513.md`

## Verification

- `cargo test -p tsz-checker string_argument_does_not_match_generic_array_overload --lib -- --nocapture`
- `cargo test -p tsz-checker overload --lib -- --nocapture`
- `cargo fmt --all --check`
