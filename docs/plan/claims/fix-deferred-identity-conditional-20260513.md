# fix(solver): accept distributive identity conditional return

- **Date**: 2026-05-13
- **Branch**: `fix-deferred-identity-conditional-20260513`
- **Base**: `upstream/main`
- **Issue**: #6064
- **PR**: https://github.com/mohsen1/tsz/pull/6124
- **Status**: ready
- **Workstream**: solver false-positive

## Intent

Fix the false TS2322 where `T` is rejected as not assignable to a transparent
conditional alias like `T extends unknown ? T : never`.

## Scope

- Reproduce the #6064 `Deferred<T>` case against `tsc` and `tsz`.
- Identify the smallest solver/checker path that should treat transparent
  identity conditionals as assignable without weakening unrelated conditional
  failures.
- Add focused regression coverage for the generic return assignment.

## Verification Plan

- `cargo fmt`
- Focused #6064 regression test
- Related conditional/assignability tests covering deferred conditional types
- Manual #6064 repro comparison against `tsc` and `tsz`

## Result

- Added a narrow target-position subtype rule for transparent identity
  conditionals of the form `T extends unknown ? T : never` and
  `T extends any ? T : never`.
- Kept non-transparent Extract-like conditionals such as
  `T extends object ? T : never` rejecting unconstrained `T`.
- Added focused checker regressions in
  `crates/tsz-checker/tests/conditional_infer_tests.rs`.

## Verification

- `cargo fmt`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker --test conditional_infer_tests distributive_identity_conditional_target_accepts_type_parameter -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker --test conditional_infer_tests extract_like_conditional_target_still_rejects_unconstrained_type_parameter -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker --test conditional_infer_tests test_no_false_ts2322_conditional_type_constraint_target -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker --test conditional_infer_tests recursive_awaited_type_parameter_assignment_keeps_type_parameter_display -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker --test conditional_infer_tests -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-solver test_deferred_conditional_target_subtyping -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-solver test_extract_pattern_assignable_to_extends_type -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-solver --lib`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker --lib`
- `env CARGO_INCREMENTAL=0 cargo check -p tsz-checker`
- `env CARGO_INCREMENTAL=0 cargo build --release -p tsz-cli --bin tsz`
- Manual #6064 repro comparison: `tsc` and `.target/release/tsz` both exit 0
  under `--noEmit --strict`
