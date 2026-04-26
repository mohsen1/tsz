# fix(checker): fall back to any/unknown when catch clause has invalid type annotation

- **Date**: 2026-04-26
- **Branch**: `fix/checker-catch-invalid-annotation-falls-back-to-any`
- **PR**: #1393
- **Status**: ready
- **Workstream**: Conformance — fingerprint parity

## Intent

When a catch clause variable has a type annotation that is neither `any` nor
`unknown` (e.g. `catch (e: number)` or `catch ({ x }: object)`), `tsc` emits
TS1196 on the annotation and treats the variable as the catch-variable default
(`any` / `unknown`) for the body. tsz currently keeps the user-provided
(invalid) type, which cascades into spurious TS2339 errors on legitimate
property access (`e.toLowerCase()`) and on destructured names (`{ x }: object`).

Mirror tsc by routing through the existing `flow_boundary::resolve_catch_variable_type`
helper whenever the annotation is invalid, both in `compute_final_type` and in
the binding-pattern path used for destructuring catch declarations.

## Files Touched

- `crates/tsz-checker/src/state/variable_checking/core.rs` (~25 LOC change in two
  branches: scalar catch declared_type and destructure pattern_type)
- `crates/tsz-checker/tests/flow_observation_boundary_tests.rs` (+2 regression tests)

## Verification

- `cargo nextest run -p tsz-checker --lib` (2886 pass, 9 skipped)
- `cargo nextest run -p tsz-checker --test flow_observation_boundary_tests` (38 pass)
- `./scripts/conformance/conformance.sh run --filter "Catch"` → 24/24 PASS
- `./scripts/conformance/conformance.sh run --filter "catchClauseWithTypeAnnotation"`
  flips from fingerprint-only failure to PASS (3 spurious TS2339 fingerprints removed).
