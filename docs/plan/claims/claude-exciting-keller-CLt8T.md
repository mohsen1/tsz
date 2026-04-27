# fix(checker): emit TS2786 for SFCs returning `undefined` with strictNullChecks

- **Date**: 2026-04-26
- **Branch**: `claude/exciting-keller-CLt8T`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

`check_jsx_component_return_type` in `crates/tsz-checker/src/checkers/jsx/extraction.rs`
stripped null/undefined from the SFC return type before checking against `JSX.Element`.
When the return type was purely nullish (e.g. `undefined`), stripping left `NEVER`, which
was treated as "unreachable → valid". The fix distinguishes NEVER-from-stripping (invalid
with strictNullChecks) from NEVER-as-the-actual-type (valid bottom type). Same fix applied
to the call-signatures path.

Root cause: `undefined` is not a valid JSX component return type with `strictNullChecks: true`.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/extraction.rs` (SFC and call-sig NEVER guard)
- `crates/tsz-checker/src/checkers/jsx/tests.rs` (3 new tests + 2 helper fns)

## Verification

- `cargo test --package tsz-checker --lib -- jsx`: 204/204 pass
- `cargo test --package tsz-checker --lib`: 2925/2925 pass
- `./scripts/conformance/conformance.sh run --filter tsxSfcReturnUndefinedStrictNullChecks`: 1/1 PASS
- Full conformance: 12210/12582 (97.0%) — net +5 vs baseline 12205, 8 improvements, 0 regressions
