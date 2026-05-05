# [WIP] fix(checker): align thisless contextual inference diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-thisless-functions-contextual-inference`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance / contextual inference and diagnostic fingerprints

## Intent

Random conformance pick selected `TypeScript/tests/cases/compiler/thislessFunctionsNotContextSensitive1.ts`.
The compact picker reports an extra `TS18046`; the verbose run shows the full
test still fails on a combination of contextual-inference false positives and
display fingerprints. This PR will root-cause the shared inference/display
rules needed to make the test match `tsc`, without adding checker-local
single-test suppressions.

Observed verbose mismatch on `origin/main`:

- Extra `TS18046` at `state123.bar2` in a Vuex-style mutation callback.
- `TS2345` fingerprints for `NonStringIterable<T>` calls render
  `NonStringIterable<unknown>` and over-report array arguments where `tsc`
  only rejects the string literal with target `never`.
- `TS2820` target display expands `ExtractFields<...>` into a literal union
  where `tsc` preserves the conditional/mapped alias surface.

## Files Touched

- TBD after root-cause analysis; likely checker contextual typing / inference
  boundary helpers and owning crate regression tests.

## Verification

- `./scripts/conformance/conformance.sh run --filter "thislessFunctionsNotContextSensitive1" --verbose` (currently failing, baseline captured)
- Planned: `cargo check --package tsz-checker`
- Planned: `cargo check --package tsz-solver`
- Planned: owning-crate `cargo nextest run` for new regression tests
- Planned: targeted conformance rerun for `thislessFunctionsNotContextSensitive1`
