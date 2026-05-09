# fix(checker): preserve instanceof union branch for partial member diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-instanceof-subtype-reduction-partial-members-ts2339`
- **PR**: #2776
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Close the fingerprint-only `typeGuardsWithInstanceOf` slice where `tsz`
reports the shared `TS2339` code but misses the two `v.onChanges` property
access diagnostics after `if (v instanceof C)`. `tsc` keeps the post-guard type
as `C | (Validator & Partial<OnChanges>)`, so accessing `onChanges` must report
against the `C` branch even though the original variable was subtype-reduced
back toward `Validator & Partial<OnChanges>`.

## Files Touched

- `crates/tsz-checker/src/flow/control_flow/core.rs`
- `crates/tsz-checker/src/error_reporter/properties.rs`
- `crates/tsz-checker/tests/conformance_issues/modules/declaration_module_emit.rs`
- `crates/tsz-solver/src/narrowing/instanceof.rs`
- `crates/tsz-solver/src/narrowing/discriminants.rs`
- `crates/tsz-solver/src/operations/property_visitor.rs`
- `crates/tsz-solver/tests/narrowing_discriminant_tests.rs`

## Verification

- `cargo fmt --all --check`
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo test -p tsz-solver narrowing_discriminant_tests::property_truthiness_narrows_union -- --nocapture`
- `cargo test -p tsz-checker --test conformance_issues test_instanceof_class_narrows_union_at_merge_point -- --nocapture`
- `./scripts/conformance/conformance.sh run --filter "typeGuardsWithInstanceOf" --verbose` (3/3 passed, 100%, 0 fingerprint-only)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed, 100%, 0 fingerprint-only)

`cargo nextest` is not installed in this environment; targeted `cargo test`
was used for local verification.

## Notes

The missed diagnostics came from several reductions combining in the
`instanceof` flow path:

- branch-label merges rebuilt antecedents through subtype-reducing unions, which
  could discard the class branch needed for later property diagnostics;
- class `instanceof` narrowing of an intersection source kept the source
  intersection on the true branch, leaving `onChanges` available where `tsc`
  keeps the class instance branch;
- property-access pruning/error construction rebuilt receiver unions through
  subtype-reducing unions, which could hide the branch that lacked the property;
- property-truthiness narrowing treated a missing property as evidence to remove
  the member, so the call site lost the diagnostic branch after the condition;
- dot-property `TS2339` emission was still gated by `noImplicitAny`, while this
  fixture only enables `strictNullChecks`.
