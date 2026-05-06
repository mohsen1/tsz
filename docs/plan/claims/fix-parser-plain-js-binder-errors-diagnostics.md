# fix(parser): align plain JS binder error diagnostics

- **Date**: 2026-05-06
- **Branch**: `fix/parser-plain-js-binder-errors-diagnostics`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the remaining missing diagnostic codes in
`TypeScript/tests/cases/conformance/salsa/plainJSBinderErrors.ts`. The prior
TS1101 slice covered strict-mode `with`; this follow-up targets the remaining
plain-JS binder/parser diagnostics reported by the quick-pick:
`TS1102`, `TS1107`, `TS1210`, `TS1214`, `TS1215`, `TS1359`, and `TS18012`.

## Files Touched

- TBD

## Verification

- `cargo nextest run` for the owning crate tests added with the fix.
- `./scripts/conformance/conformance.sh run --filter "plainJSBinderErrors" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
