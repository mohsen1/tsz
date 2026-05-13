# Fix mixin heritage diagnostics for computed class bases

- **Date**: 2026-05-13
- **Branch**: `fix-mixin-heritage-diagnostics-6249`
- **PR**: #6258
- **Status**: ready
- **Workstream**: checker correctness / public issue #6249

## Intent

Close #6249 by restoring TypeScript-compatible diagnostics for classes that
extend a mixin function result. The fix detects computed call-expression
heritage whose mixin type-parameter constructor constraint is invalid for TS2545,
then preserves the expected TS2510/TS2417 cascade and avoids leaking invalid
base instance properties into the derived class instance.

## Files Touched

- `crates/tsz-checker/src/state/type_resolution/constructors.rs`
- `crates/tsz-checker/src/state/state_checking/heritage.rs`
- `crates/tsz-checker/src/classes/class_checker.rs`
- `crates/tsz-checker/tests/mixin_base_no_member_no_ts2416_tests.rs`
- `docs/plan/claims/fix-mixin-heritage-diagnostics-6249.md`

## Verification

- `cargo test -p tsz-checker --test mixin_base_no_member_no_ts2416_tests mixin_call_heritage_reports_static_return_and_property_diagnostics -- --nocapture` (1 passed)
- `cargo test -p tsz-checker --test mixin_base_no_member_no_ts2416_tests -- --nocapture` (6 passed)
- `cargo test -p tsz-checker --lib ts2545 -- --nocapture` (3 passed)
- `cargo test -p tsz-checker --lib ts2515_expression_based_heritage -- --nocapture` (1 passed)
- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo clippy --profile ci-lint -p tsz-checker --all-targets -- -D warnings`
