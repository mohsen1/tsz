# fix(checker): suppress TS2786 for SFCs whose return type is never

- **Date**: 2026-04-26
- **Branch**: `fix/jsx-ts2786-never-sfc-return`
- **PR**: #1442
- **Status**: ready
- **Workstream**: 1 (conformance)

## Intent

`check_jsx_component_return_type` was emitting a spurious TS2786
("'X' cannot be used as a JSX component") for SFCs whose return type
strips to `never` after nullish removal — e.g.
`function MyComp(props) { return null!; }`. `null!` narrows to `never`,
which is the bottom type and IS assignable to `JSX.Element`. tsc
accepts this and emits no TS2786. Aligns the SFC branch with the
construct-signature branch (which already treats `stripped == NEVER`
as valid) and with `check_jsx_sfc_return_type` (which already early-
exits on `never`).

Fixes conformance test
`TypeScript/tests/cases/compiler/spellingSuggestionJSXAttribute.tsx`
(removes spurious TS2786 from actual codes; brings the test from
`only-extra` failure to error-code-level parity).

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/extraction.rs` (~10 LOC)
- `crates/tsz-checker/src/checkers/jsx/tests.rs` (regression test)
- `scripts/conformance/conformance-baseline.txt` (drop spurious TS2786)

## Verification

- `cargo nextest run -p tsz-checker --lib jsx_` (199 tests pass)
- `./scripts/conformance/conformance.sh run --filter "spellingSuggestionJSXAttribute" --verbose` (codes match expected)
