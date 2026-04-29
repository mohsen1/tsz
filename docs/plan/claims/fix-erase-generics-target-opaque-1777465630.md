# fix(solver): keep target type params opaque for non-generic source assignability

- **Date**: 2026-04-29
- **Branch**: `fix/erase-generics-target-opaque-1777465630`
- **PR**: #1788
- **Status**: ready
- **Workstream**: Conformance (TS2322 family)

## Intent

When checking assignability of a non-generic callable/construct signature
source into a generic target (`b7 = a7` where `b7: <T extends Base>(...)`
and `a7: (...)`), the solver currently erases target type parameters to
their constraints. This loses opacity: source's concrete `Base` arg can
match target's `(arg: T)` after T->Base substitution, even though tsc
rejects the assignment with TS2322 ("'Base' is assignable to the
constraint of type 'T', but 'T' could be instantiated with a different
subtype").

The root cause is in
`crates/tsz-solver/src/relations/subtype/rules/functions/checking.rs`
around line 511-521 where `erase_generics=true` paths use
`erase_type_params_to_constraints`. tsc's `getRestrictiveTypeParameter`
behavior keeps target type params opaque (same constraint, fresh
identity); the right surgical fix is to skip the constraint substitution
and rely on the existing concrete-vs-type-parameter rejection in the
subtype core.

## Files Touched

- `crates/tsz-solver/src/relations/subtype/rules/functions/checking.rs`
- `crates/tsz-solver/tests/callable_tests.rs` (one stale test that
  asserted incorrect behavior; updated to match tsc, plus a regression
  test for the `b7 = a7` pattern)

## Verification

- `cargo fmt --check`
- `cargo check --package tsz-solver`
- `cargo check --package tsz-checker`
- `cargo nextest run --package tsz-solver test_nongeneric_construct_sig_not_assignable_to_generic_target test_nongeneric_construct_sig_nested_callback_not_assignable_to_generic_target`
- `cargo test --package tsz-checker --test signature_assignability_regression_tests -- --nocapture`
- `./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --filter assignmentCompatWithConstructSignatures4 --verbose` (1/1 passed)
