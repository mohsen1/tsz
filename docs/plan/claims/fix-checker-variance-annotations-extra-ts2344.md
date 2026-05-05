# fix(checker): suppress variance annotation constraint cascade

- **Date**: 2026-05-05
- **Branch**: `fix/checker-variance-annotations-extra-ts2344`
- **PR**: #3322
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the only-extra conformance failure in
`TypeScript/tests/cases/conformance/types/typeParameters/typeParameterLists/varianceAnnotations.ts`.
tsc reports the expected variance annotation diagnostics without an additional
generic constraint failure, while tsz currently emits an extra `TS2344`.

The implementation also aligns unsupported variance annotations on type alias
bodies: tsz now reports TS2637 for each annotated parameter on unsupported
alias body forms and skips follow-on TS2636 variance validation for those
unsupported bodies. Malformed variance keyword names still defer to parser
diagnostics so `varianceAnnotationsWithCircularlyReferencesError.ts` stays
clean.

## Files Touched

- `crates/tsz-checker/src/checkers/generic_checker/mod.rs`
- `crates/tsz-checker/src/checkers/generic_checker/instantiation_expression_constraints.rs`
- `crates/tsz-checker/src/types/type_checking/type_alias_checking.rs`
- `crates/tsz-checker/src/tests/dispatch_tests.rs`
- `crates/tsz-checker/tests/conformance_issues/core/helpers.rs`
- `docs/plan/claims/fix-checker-variance-annotations-extra-ts2344.md`

## Verification

- `cargo fmt --check`
- `CARGO_TARGET_DIR=/tmp/tsz-codex-next28-target CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo nextest run -j 1 -p tsz-checker --lib ts2344_parenthesized_typeof_instantiation_does_not_emit_constraint_diagnostic --no-tests=fail` (1/1 PASS)
- `CARGO_TARGET_DIR=/tmp/tsz-codex-next28-target CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo nextest run -j 1 -p tsz-checker --test conformance_issues test_variance_annotations_require_direct_supported_type_alias_bodies --no-tests=fail` (1/1 PASS)
- `CARGO_TARGET_DIR=/tmp/tsz-codex-next28-target CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 ./scripts/conformance/conformance.sh run --filter "varianceAnnotations" --verbose` (1/2 PASS; `varianceAnnotationsWithCircularlyReferencesError.ts` PASS; `varianceAnnotations.ts` error-code set matches, remaining fingerprint-only drift is TS2322/TS2345 display/position)

## Conformance Impact

- Removes the extra TS2344 from the `InstanceType<(typeof Anon<T>)>` case in
  `varianceAnnotations.ts`.
- Keeps `varianceAnnotationsWithCircularlyReferencesError.ts` passing while
  preserving expected TS2637 fingerprints for malformed modifier-order cases.
