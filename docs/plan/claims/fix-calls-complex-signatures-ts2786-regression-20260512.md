# fix(checker): restore callsOnComplexSignatures JSX union validity

- **Date**: 2026-05-12
- **Branch**: `fix/calls-complex-signatures-ts2786-regression-20260512`
- **Base**: `main`
- **PR**: https://github.com/mohsen1/tsz/pull/5780
- **Status**: ready
- **Workstream**: conformance

## Intent

Restore the `callsOnComplexSignatures.tsx` conformance pass on current `main`.
The earlier fix in PR #5679 merged, but a later change reintroduced an extra
TS2786 for a JSX tag-name union case while tsc expects no diagnostics.

## Scope

- Reproduce the focused `callsOnComplexSignatures` delta.
- Fix the smallest JSX component validity regression without weakening invalid
  component diagnostics such as `jsxComponentTypeErrors`.
- Add or update focused regression coverage in `tsz-checker` if the root cause is
  isolated.

## Verification Plan

- `./scripts/conformance/conformance.sh run --filter "callsOnComplexSignatures" --verbose`
- Relevant JSX regression tests in `tsz-checker`
- Guard conformance for `jsxComponentTypeErrors`
- `cargo fmt --all`
- Pre-commit or equivalent direct-crate validation before marking ready

## Progress

- Restored the JSX union component return-type guard for union members that are
  already accepted by JSX props extraction.
- Verified the focused conformance regression now matches tsc.
- Verified the existing invalid JSX union guard still emits TS2786.

## Verification

- `cargo fmt --all` - passed
- `cargo test -p tsz-checker jsx_union_component_with_invalid_return_emits_ts2786 -- --nocapture` - passed
- `cargo test -p tsz-checker jsx_react_component_type_union_does_not_emit_ts2786 -- --nocapture` - passed
- `cargo test -p tsz-checker jsx_union_of_invalid_function_and_class_component_emits_ts2786 -- --nocapture` - passed
- `./scripts/conformance/conformance.sh run --filter "callsOnComplexSignatures" --verbose` - `FINAL RESULTS: 1/1 passed (100.0%)`
- `./scripts/conformance/conformance.sh run --filter "jsxComponentTypeErrors" --verbose` - `FINAL RESULTS: 1/1 passed (100.0%)`
