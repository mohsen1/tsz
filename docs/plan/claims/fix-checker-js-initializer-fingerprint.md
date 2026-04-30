# fix(checker): align JS initializer diagnostic fingerprints

- **Date**: 2026-04-29
- **Timestamp**: 2026-04-29 21:53:00 UTC
- **Branch**: `fix/checker-js-initializer-fingerprint`
- **PR**: #1830
- **Status**: ready
- **Workstream**: 1 - Diagnostic Conformance And Fingerprints

## Intent

Picked by `scripts/session/quick-pick.sh` on 2026-04-29. The target
`TypeScript/tests/cases/conformance/salsa/typeFromJSInitializer.ts` is a
fingerprint-only mismatch with matching diagnostic codes (`TS2322`, `TS7006`,
and `TS7008`) but divergent fingerprint details. The root cause was that
checked-JS implicit-any constructor members and nullish-initialized locals could
use their flow-narrowed read type as the write surface after earlier
assignments. TypeScript still reports the implicit-any diagnostic, but keeps
those writes assignable through `any`/`any[]`.

## Files Touched

- `crates/tsz-checker/src/assignability/assignment_checker/assignment_ops.rs`
- `crates/tsz-checker/src/types/computation/assignment_target.rs`
- `crates/tsz-checker/src/types/computation/mod.rs`
- `crates/tsz-checker/src/types/class_type/js_class_properties.rs`
- `crates/tsz-checker/src/types/computation/helpers.rs`
- `crates/tsz-checker/tests/js_constructor_property_tests.rs`
- `docs/plan/claims/fix-checker-js-initializer-fingerprint.md`

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --test js_constructor_property_tests`
- `./scripts/conformance/conformance.sh run --filter "typeFromJSInitializer" --verbose`
  - `FINAL RESULTS: 4/4 passed (100.0%)`
  - `Fingerprint-only: 0`
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run --package tsz-checker --lib`
- `cargo nextest run --package tsz-solver --lib`
- `./scripts/conformance/conformance.sh run --max 200`
  - `FINAL RESULTS: 200/200 passed (100.0%)`
  - `Fingerprint-only: 0`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run`
  - `FINAL RESULTS: 12271/12582 passed (97.5%)`
  - `Known failures: 18`
  - `Fingerprint-only: 197`
  - `TypeScript/tests/cases/conformance/salsa/typeFromJSInitializer.ts` listed as `FAIL -> PASS`
  - `Net: 12235 -> 12271 (+36)`
