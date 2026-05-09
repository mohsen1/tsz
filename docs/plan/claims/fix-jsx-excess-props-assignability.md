# [WIP] fix(checker): restore JSX excess props assignability diagnostic

- **Date**: 2026-05-05
- **Branch**: `fix/jsx-excess-props-assignability`
- **PR**: #2745
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the conformance divergence in `jsxExcessPropsAndAssignability.tsx`.
The picker selected an only-missing failure where `tsc` emits `TS2322` and
`TS2698`, while `tsz` emitted only `TS2698`.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/extraction.rs`
- `crates/tsz-checker/src/checkers/jsx/props/generic_spread.rs`
- `crates/tsz-checker/src/checkers/jsx/props/mod.rs`
- `crates/tsz-checker/src/checkers/jsx/props/resolution.rs`
- `crates/tsz-checker/tests/jsx_component_attribute_tests.rs`

## Verification

- `cargo nextest run -p tsz-checker --test jsx_component_attribute_tests test_jsx_excess_props_and_assignability_react16_fixture_matches_tsc`
- `cargo nextest run -p tsz-checker --test jsx_component_attribute_tests`
- `./.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz-worktrees/jsx-excess-props-assignability/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary ./.target/dist-fast/tsz --filter jsxExcessPropsAndAssignability --workers 1 --verbose --print-fingerprints`
- Full pre-commit hook before push: clippy, wasm rustc warning gate, architecture guardrails, and `14898` affected tests passed.
