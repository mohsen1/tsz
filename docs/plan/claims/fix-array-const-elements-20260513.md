# fix(solver): preserve const-asserted array element literals

- **Date**: 2026-05-13
- **Branch**: `fix-array-const-elements-20260513`
- **Base**: `upstream/main`
- **Issue**: #6112
- **PR**: https://github.com/mohsen1/tsz/pull/6119
- **Status**: ready
- **Workstream**: solver false-positive

## Intent

Fix the false TS2322 where array literals containing individually
const-asserted elements are inferred as widened primitive arrays instead of
literal-union arrays.

## Scope

- Reproduce the #6112 `["a" as const, "b" as const, "c" as const]` case
  against `tsc` and `tsz`.
- Keep implicit literal-array widening behavior intact for unasserted elements.
- Add focused checker/solver regression coverage for the const-asserted array
  element case.

## Verification Plan

- `cargo fmt`
- Focused regression test for #6112
- Related array-literal/generic-inference tests that cover widening behavior
- Manual #6112 repro comparison against `tsc` and `tsz`

## Result

- Added array-literal element tracking so the BCT path preserves literal
  element types only when every array-context element is explicitly
  const-asserted.
- Kept holes, spreads, mixed const/unasserted elements, and ordinary arrays on
  the existing widening path.
- Added regression coverage in
  `crates/tsz-checker/tests/tuple_type_assertion_inference_tests.rs` for both
  #6112 and the mixed widening guard.

## Verification

- `cargo fmt`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker --test tuple_type_assertion_inference_tests -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker --test generic_call_inference_tests const_type_param_nested_array_in_object_no_false_ts2322 -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-solver test_widen_array_of_literals_widens_element -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker --lib`
- `env CARGO_INCREMENTAL=0 cargo check -p tsz-checker`
- `env CARGO_INCREMENTAL=0 cargo build --release -p tsz-cli --bin tsz`
- Manual #6112 repro comparison: `tsc` and `.target/release/tsz` both exit 0
  under `--noEmit --strict`
