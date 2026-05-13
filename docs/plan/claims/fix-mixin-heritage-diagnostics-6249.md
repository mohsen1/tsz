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
- `crates/tsz-checker/src/state/type_resolution/mixin_constraints.rs`
- `crates/tsz-checker/src/state/type_resolution/mod.rs`
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

## CI follow-up

- Merged current `origin/main` into the branch after #6263 landed.
- Split mixin-constraint helpers out of `constructors.rs`, reducing it to 1985
  lines and satisfying the checker file-size ceiling.
- Scoped TS2417/static-side fallback to the invalid mixin-call path so ordinary
  constructor intersections keep their synthesized instance intersection.

Additional verification:

- `cargo test -p tsz-checker --test override_intersection_display_tests override_intersection_shows_named_types -- --nocapture` (pass)
- `cargo test -p tsz-checker --test override_intersection_display_tests override_intersection_mixed_named_and_anonymous -- --nocapture` (pass)
- `cargo test -p tsz-core checker_state_tests::test_class_extends_intersection_type_ts2339 -- --nocapture` (pass)
- `cargo test -p tsz-core checker_state_tests::test_class_extends_class_like_constructor_properties -- --nocapture` (pass)
- `cargo test -p tsz-checker --test override_intersection_display_tests -- --nocapture` (3 passed)
- `cargo test -p tsz-checker --test mixin_base_no_member_no_ts2416_tests -- --nocapture` (6 passed)
- `cargo test -p tsz-checker architecture_contract_tests_src::test_checker_file_size_ceiling -- --nocapture` (pass; remaining filtered binaries all passed)
