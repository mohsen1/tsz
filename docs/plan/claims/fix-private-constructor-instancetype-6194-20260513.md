# fix(checker): reject InstanceType on private constructors

- **Date**: 2026-05-13
- **Branch**: `fix-private-constructor-instancetype-6194-20260513`
- **PR**: #6240
- **Status**: ready
- **Workstream**: Diagnostic conformance

## Intent

Close #6194 by making constructor accessibility participate in generic constraint validation for utility types such as `InstanceType<T extends abstract new (...args: any) => any>`. A class value with a private or protected constructor must not satisfy a public/abstract construct-signature constraint, even though it has a construct signature structurally.

## Files Touched

- `docs/plan/claims/fix-private-constructor-instancetype-6194-20260513.md`
- `crates/tsz-checker/src/checkers/generic_checker/constraint_validation.rs`
- `crates/tsz-checker/src/checkers/generic_checker/instantiation_expression_constraints.rs`
- `crates/tsz-checker/src/classes/constructor_checker.rs`
- `crates/tsz-checker/src/tests/dispatch_tests.rs`
- `crates/tsz-checker/tests/ts2344_class_constructor_constraint.rs`

## Verification

- `cargo test -p tsz-checker --lib dispatch_tests::instancetype_private_constructor_constraint_violation_emits_ts2344 -- --nocapture` (1 passed)
- `cargo test -p tsz-checker --test ts2344_class_constructor_constraint constructor_emits_ts2344 -- --nocapture` (3 passed)
- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo run -p tsz-cli --bin tsz -- --noEmit <#6194 repro>` (emits TS2344)
