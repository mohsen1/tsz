# fix(checker): align destructuring rest property diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-destructuring-unspreadable-rest-diagnostics`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance

## Intent

Claim the conformance target `TypeScript/tests/cases/compiler/destructuringUnspreadableIntoRest.ts`.
The current checker emits the right TS2339 codes, but formats object-rest receiver
types from class `this` expressions as structural `{}` or `{ publicProp: string; }`
instead of TypeScript's `Omit<this, ...>` surfaces. This PR will preserve the
object-rest Omit display for those class-shaped receivers while keeping concrete
object rest behavior unchanged.

## Files Touched

- `docs/plan/claims/fix-checker-destructuring-unspreadable-rest-diagnostics.md`
- `crates/tsz-checker/src/state/variable_checking/binding_rest.rs` (planned)
- checker regression tests (planned)

## Verification

- Baseline: `./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --filter "destructuringUnspreadableIntoRest" --verbose` (fingerprint-only TS2339 mismatch)
