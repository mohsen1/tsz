# fix(checker): align co/contravariant inference diagnostics

- **Date**: 2026-04-29
- **Timestamp**: 2026-04-29 21:28:26 UTC
- **Branch**: `fix/checker-co-contra-inference-fingerprint`
- **PR**: #1827
- **Status**: ready
- **Workstream**: 1 - Diagnostic Conformance And Fingerprints

## Intent

Picked by `scripts/session/quick-pick.sh` on 2026-04-29. The target
`TypeScript/tests/cases/compiler/coAndContraVariantInferences3.ts` had stacked
extra diagnostics: a false `TS2344` for `Parameters<{ [P in K]: T[P] }[K]>`
where `T` has a callable numeric index signature, followed by provisional
`TS7031` binding-element diagnostics leaking from generic object-literal
contextual typing. This PR teaches the checker to recognize callable mapped
numeric-key indexed access through constraints and to roll back provisional
implicit-any diagnostics under unresolved generic object-literal contexts.

## Files Touched

- `crates/tsz-checker/src/checkers/call_checker/candidate_collection.rs`
- `crates/tsz-checker/src/checkers/generic_checker/constraint_validation.rs`
- `crates/tsz-checker/src/types/computation/object_literal_context.rs`
- `crates/tsz-checker/tests/ts2344_keyof_bare_tparam_defer_tests.rs`
- `docs/plan/claims/fix-checker-co-contra-inference-fingerprint.md`

## Verification

- `cargo fmt --check`
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run -p tsz-checker --test ts2344_keyof_bare_tparam_defer_tests` (6 passed)
- `cargo nextest run --package tsz-checker --lib` (3023 passed, 10 skipped)
- `cargo nextest run --package tsz-solver --lib` (5554 passed, 9 skipped)
- `./scripts/conformance/conformance.sh run --filter "coAndContraVariantInferences3" --verbose` (1/1 passed)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run` (12271/12582 passed)
