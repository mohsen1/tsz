# fix(solver): distinguish symbol index signatures from string slots

- **Date**: 2026-05-03
- **Branch**: `fix/next-conformance-fingerprint-05032005`
- **PR**: #2625
- **Status**: ready for review
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Close the symbol-index slice of the `indexSignatures1` conformance mismatch.
`tsz` currently stores symbol index signatures in the object shape's
`string_index` slot, so indexed access and subtype checks sometimes treat
`[key: symbol]` as `[key: string]`. That creates false `TS7052`/`TS2722`
cascades for unique-symbol keys and lets symbol-named properties participate in
string-index compatibility checks.

## Files Touched

- `crates/tsz-solver/src/evaluation/evaluate_rules/index_access.rs`
- `crates/tsz-solver/src/objects/index_signatures.rs`
- `crates/tsz-solver/src/relations/subtype/rules/objects.rs`
- `crates/tsz-solver/src/type_queries/extended.rs`
- `crates/tsz-solver/tests/index_access_comprehensive_tests.rs`

## Verification

- `cargo fmt -p tsz-solver --check`
- `cargo check -p tsz-solver`
- `cargo nextest run -p tsz-solver -E 'test(~symbol_index)'`
- `cargo nextest run -p tsz-solver -E 'test(~symbol_named)'`
- `cargo nextest run -p tsz-solver`
- `./scripts/conformance/conformance.sh run --filter "indexSignatures1" --verbose`
  still fails on unrelated template-pattern, duplicate-index, and branded-string
  gaps; the targeted `TS7052`/`TS2722` symbol-index cascade is removed.
