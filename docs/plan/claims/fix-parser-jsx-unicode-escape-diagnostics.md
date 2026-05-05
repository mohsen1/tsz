# fix(parser): suppress cascades for JSX unicode escape diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/parser-jsx-unicode-escape-diagnostics`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the `only-extra` conformance divergence in
`TypeScript/tests/cases/conformance/jsx/unicodeEscapesInJsxtags.tsx`.

`tsc` reports `TS17021` for unicode escape sequences in JSX tag and attribute
names. `tsz` reports those diagnostics, but also emits parser/checker cascades
such as `TS17002`, `TS2304`, and `TS2339`, and several `TS17021` spans differ.
This slice will align the JSX unicode escape diagnostic surface without
weakening ordinary JSX name checking.

## Files Touched

- TBD

## Verification

- Baseline targeted conformance:
  - `./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --filter "unicodeEscapesInJsxtags" --verbose`
  - Current result: `only-extra`; expected `TS17021`, actual
    `TS2304`, `TS2339`, `TS17002`, and `TS17021`.
