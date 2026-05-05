# fix(checker): align destructuring rest property diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-destructuring-unspreadable-rest-diagnostics`
- **PR**: #3230
- **Status**: ready
- **Workstream**: conformance

## Intent

Claim the conformance target `TypeScript/tests/cases/compiler/destructuringUnspreadableIntoRest.ts`.
The current checker emits the right TS2339 codes, but formats object-rest receiver
types from class `this` expressions as structural `{}` or `{ publicProp: string; }`
instead of TypeScript's `Omit<this, ...>` surfaces. This PR will preserve the
object-rest Omit display for those direct-`this` rest bindings while keeping
the underlying structural rest types unchanged.

## Files Touched

- `docs/plan/claims/fix-checker-destructuring-unspreadable-rest-diagnostics.md`
- `crates/tsz-checker/src/error_reporter/properties.rs`
- `crates/tsz-checker/src/state/variable_checking/binding_rest.rs`
- `crates/tsz-checker/tests/destructuring_rest_omit_unspreadable_tests.rs`

## Verification

- Baseline: `./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --filter "destructuringUnspreadableIntoRest" --verbose` (fingerprint-only TS2339 mismatch)
- `cargo fmt --check`
- `CARGO_INCREMENTAL=0 cargo nextest run -p tsz-checker rest_from_class_this_uses_omit_display_for_missing_rest_properties`
- `./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --filter "destructuringUnspreadableIntoRest" --verbose` (1/1 passed)
- `./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --max 200` (200/200 passed)
