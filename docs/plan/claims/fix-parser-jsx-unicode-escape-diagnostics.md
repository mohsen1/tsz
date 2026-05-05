# fix(parser): suppress cascades for JSX unicode escape diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/parser-jsx-unicode-escape-diagnostics`
- **PR**: #3161
- **Status**: ready
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

- `crates/tsz-scanner/src/scanner_impl.rs`
  - Preserve decoded JSX identifier text when hyphenated names contain unicode
    escapes, including names that start with an escape and then continue with a
    hyphen segment.
- `crates/tsz-parser/src/parser/state_types_jsx.rs`
  - Run the JSX unicode-escape diagnostic check for property-access tag
    segments such as `<x.\u0076ideo />`.
- `crates/tsz-checker/src/checkers/jsx/orchestration/resolution.rs`
- `crates/tsz-checker/src/checkers/jsx/orchestration/component_props.rs`
  - Suppress JSX semantic cascades once the source contains JSX unicode escape
    parse errors, matching the `TS17021`-only baseline surface.
- `crates/tsz-parser/tests/jsx_namespace_recovery_tests.rs`
  - Pin `TS17021` spans for hyphenated JSX tag/attribute names and property
    access segments.

## Verification

- Baseline targeted conformance:
  - `./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --filter "unicodeEscapesInJsxtags" --verbose`
  - Current result: `only-extra`; expected `TS17021`, actual
    `TS2304`, `TS2339`, `TS17002`, and `TS17021`.
- Fixed targeted conformance:
  - `./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --filter "unicodeEscapesInJsxtags" --verbose`
  - Result: `1/1 passed (100.0%)`.
- Focused parser tests:
  - `cargo nextest run -p tsz-parser jsx_hyphenated_unicode_escape_reports_from_full_name_start`
  - Result: passed.
- Formatting:
  - `cargo fmt --check`
  - Result: passed.
- Smoke conformance:
  - `./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --max 200`
  - Result: `200/200 passed (100.0%)`.
