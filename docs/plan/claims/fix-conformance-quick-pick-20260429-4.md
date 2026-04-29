# fix(checker): align recursive complicated class fingerprints

- **Date**: 2026-04-29
- **Branch**: `fix/conformance-quick-pick-20260429-4`
- **PR**: #1809
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance and fingerprints)

## Intent

Targeted the random conformance pick `TypeScript/tests/cases/compiler/recursiveComplicatedClasses.ts`.
The root cause was duplicate-identifier emission only anchoring the local
`class Symbol` declaration, while `tsc` also reports the colliding default-lib
`Symbol` declarations. This PR adds a narrow TS2300 default-lib remote-location
emission path.

## Files Touched

- `crates/tsz-checker/src/types/type_checking/duplicate_identifiers_helpers.rs`
- `crates/tsz-checker/tests/ts2300_tests.rs`

## Verification

- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run -p tsz-checker --test ts2300_tests -E 'test(duplicate_identifier_with_default_lib_symbol_reports_lib_locations)'`
- `cargo nextest run --package tsz-checker --lib` (3006 passed, 10 skipped)
- `./scripts/conformance/conformance.sh run --filter "recursiveComplicatedClasses" --verbose` (1/1 passed)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run` (12246/12582 passed, net +11; includes `recursiveComplicatedClasses.ts` FAIL -> PASS)
