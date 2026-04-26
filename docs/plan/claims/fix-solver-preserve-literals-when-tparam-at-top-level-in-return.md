# fix(solver): preserve literal candidates when type param appears at top level in return type

- **Date**: 2026-04-26
- **Branch**: `fix/solver-preserve-literals-when-tparam-at-top-level-in-return`
- **PR**: TBD
- **Status**: claim
- **Workstream**: Conformance — inference literal preservation

## Intent

Match tsc's `widenLiteralTypes` rule from `getCovariantInference`: do not
widen literal candidates when the type parameter occurs at top-level in
the signature return type and the variable is not yet fixed. Fixes the
fingerprint mismatch on `maxConstraints.ts` where tsz reports
`Comparable<number>` instead of `Comparable<1 | 2>`.

## Files Touched

- `crates/tsz-solver/src/inference/infer.rs` (add return_type tracking)
- `crates/tsz-solver/src/inference/infer_resolve.rs` (extend
  `preserve_literals` decision; helper for top-level type-param check)
- `crates/tsz-solver/src/operations/generic_call/resolve.rs` (set the
  signature return type on the inference context)
- regression test under `crates/tsz-solver/tests/`

## Verification

- `./scripts/conformance/conformance.sh run --filter "maxConstraints" --verbose`
- `cargo nextest run -p tsz-solver`
- `cargo nextest run -p tsz-checker`
