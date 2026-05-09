# [WIP] fix(parser): align JSX invalid Esprima fingerprint

- **Date**: 2026-05-04
- **Branch**: `fix/parser-jsx-invalid-esprima-fingerprint`
- **PR**: #2714
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Investigate and fix the fingerprint-only diagnostic mismatch in
`TypeScript/tests/cases/conformance/jsx/jsxInvalidEsprimaTestSuite.tsx`.
The codes already match TypeScript, so this slice is scoped to parser recovery
diagnostic positions/messages for invalid JSX syntax.

## Files Touched

- `crates/tsz-parser/src/parser/state.rs`
- `crates/tsz-parser/src/parser/state_declarations_exports.rs`
- `crates/tsz-parser/src/parser/state_expressions.rs`
- `crates/tsz-parser/src/parser/state_types_jsx.rs`
- `docs/plan/claims/fix-parser-jsx-invalid-esprima-fingerprint.md`

## Verification

- Reproduced: `./.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary ./.target/dist-fast/tsz --filter jsxInvalidEsprimaTestSuite --workers 1 --verbose --print-fingerprints`
- Fixed: same command passes `1/1`, with `Fingerprint-only: 0`.
- Parser check: `cargo check -p tsz-parser`
- Parser tests: `cargo test -p tsz-parser jsx`
