# fix(checker): align contextual signature instantiation fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-contextual-signature-instantiation-fingerprints`
- **PR**: https://github.com/mohsen1/tsz/pull/2924
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the current fingerprint-only divergence in
`TypeScript/tests/cases/conformance/types/typeRelationships/typeInference/contextualSignatureInstantiation.ts`.
The picker reports matching diagnostic codes (`TS2345`, `TS2403`), so this PR
will root-cause the remaining diagnostic message, span, count, or ordering
mismatch without colliding with the stale merged claim from PR #1929.

## Resolution

Preserve TSC's contextual signature instantiation rejection when a generic
callback's single naked type parameter receives disjoint contextual parameter
candidates. The existing guard caught `foo(g)` but a later generic-call
normalization path still accepted the two `bar(..., g)` calls and widened their
expected callback return to `string | number`. This PR detects the conflict
after final generic substitution and reports the TSC-shaped callback type with
the first conflicting candidate as the return type.

## Files Touched

- `docs/plan/claims/fix-checker-contextual-signature-instantiation-fingerprints.md`
- `crates/tsz-solver/src/operations/generic_call/inference_helpers.rs`
- `crates/tsz-solver/src/operations/generic_call/resolve.rs`
- `crates/tsz-checker/tests/generic_call_inference_tests.rs`

## Verification

- `cargo test -p tsz-checker --test generic_call_inference_tests contextual_signature_instantiation_rejects_conflicting_generic_params`
- `./scripts/conformance/conformance.sh run --filter "contextualSignatureInstantiation" --verbose` (6/6)
- `./scripts/conformance/conformance.sh run --max 200` (200/200)
- `scripts/githooks/pre-commit`
