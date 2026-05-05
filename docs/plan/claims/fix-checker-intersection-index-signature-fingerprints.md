# fix(checker): align intersection index signature fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-intersection-index-signature-fingerprints`
- **PR**: #3261
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only mismatch in
`TypeScript/tests/cases/conformance/types/intersection/intersectionWithIndexSignatures.ts`.
The diagnostic codes and spans already match tsc, but tsz reports aliased
types (`A`, `A & B`) where tsc expands the index-signature value types
(`{ a: string }`, `{ a: string; b: string }`) for the #32484 repro.

## Files Touched

- `docs/plan/claims/fix-checker-intersection-index-signature-fingerprints.md`
- `crates/tsz-checker/Cargo.toml`
- `crates/tsz-checker/src/state/state_checking/source_file.rs`
- `crates/tsz-checker/src/types/computation/binary.rs`
- `crates/tsz-checker/tests/intersection_index_signature_fingerprint_tests.rs`
- `crates/tsz-core/src/config/mod.rs`
- `crates/tsz-core/src/module_resolver/request_types.rs`
- `crates/tsz-solver/src/diagnostics/format/compound.rs`

## Verification

- `./scripts/conformance/conformance.sh run --filter "intersectionWithIndexSignatures" --verbose`
  - `FINAL RESULTS: 1/1 passed (100.0%)`
- `CARGO_TARGET_DIR=.target/nextest-local cargo nextest run -p tsz-checker --test intersection_index_signature_fingerprint_tests`
- `./scripts/conformance/conformance.sh run --max 200`
  - `FINAL RESULTS: 200/200 passed (100.0%)`
- `PATH="$HOME/.cargo/bin:$PATH" scripts/githooks/pre-commit`
  - `All pre-commit checks passed!`
