# fix(checker): align required mapped type variance diagnostics

- **Date**: 2026-05-07
- **Branch**: `fix/conformance-next-20260507-063831`
- **PR**: #4341
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the remaining fingerprint-only conformance failure for
`TypeScript/tests/cases/compiler/requiredMappedTypeModifierTrumpsVariance.ts`.
`tsc` and `tsz` already agree on the diagnostic codes (`TS2322`, `TS2339`,
`TS2741`), but one or more diagnostic fingerprints still differ. An older
ready claim fixed the TS2339 receiver display for this fixture; this slice is
scoped to the remaining mismatch.

## Files Touched

- `crates/tsz-checker/src/types/computation/object_literal_context.rs`
- `crates/tsz-checker/src/tests/dispatch_tests.rs`
- `scripts/conformance/conformance-baseline.txt`
- `scripts/conformance/conformance-detail.json`
- `scripts/conformance/conformance-snapshot.json`

## Verification

- `cargo nextest run -p tsz-checker nested_mapped_application_property_preserves_literal_context`
- `./scripts/conformance/conformance.sh run --filter "requiredMappedTypeModifierTrumpsVariance" --verbose`
- `cargo fmt --check && ./scripts/conformance/conformance.sh run --max 200`
- Pre-commit hook: clippy, wasm rustc warnings, architecture guardrails, and 16,089 affected-crate tests
- `./scripts/conformance/conformance.sh snapshot` -> `12582 tests, 12474 passed, 108 failed`
