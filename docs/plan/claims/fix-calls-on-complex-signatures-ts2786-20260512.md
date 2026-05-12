# Claim: callsOnComplexSignatures TS2786 regression

- **Date**: 2026-05-12
- **Branch**: `fix/calls-on-complex-signatures-ts2786-20260512`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Conformance regression)

## Intent

Current `main` regressed `TypeScript/tests/cases/compiler/callsOnComplexSignatures.tsx`: tsc expects no diagnostics, while tsz emits an extra TS2786 for the JSX tag-name union case in `test5`.

Fix the JSX component validity path so valid `React.ComponentType<P1> | React.ComponentType<P2>` component variables do not report TS2786, while preserving the recently fixed TS2786 diagnostics for invalid union component return types.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/extraction.rs`
- `crates/tsz-checker/src/checkers/jsx/tests.rs`

## Verification

- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run --filter callsOnComplexSignatures --verbose`
  - `FINAL RESULTS: 1/1 passed (100.0%)`
- `.target/dist-fast/tsz-conformance --filter jsxComponentTypeErrors --cache-file scripts/conformance/tsc-cache-full.json`
  - `FINAL RESULTS: 1/1 passed (100.0%)`
- `.target/dist-fast/tsz-conformance --filter tsxTypeArgumentPartialDefinitionStillErrors --cache-file scripts/conformance/tsc-cache-full.json`
  - `FINAL RESULTS: 1/1 passed (100.0%)`
- `.target/dist-fast/tsz-conformance --filter inlineJsxFactoryDeclarationsLocalTypes --cache-file scripts/conformance/tsc-cache-full.json`
  - `FINAL RESULTS: 1/1 passed (100.0%)`
- `.target/dist-fast/tsz-conformance --filter tsxElementResolution10 --cache-file scripts/conformance/tsc-cache-full.json`
  - `FINAL RESULTS: 1/1 passed (100.0%)`
- `.target/dist-fast/tsz-conformance --filter tsxSfcReturnUndefinedStrictNullChecks --cache-file scripts/conformance/tsc-cache-full.json`
  - `FINAL RESULTS: 1/1 passed (100.0%)`
- `cargo test -p tsz-checker jsx_react_ -- --nocapture`
  - passed, including 6 matching JSX tests
- `cargo fmt --all --check`
  - passed
- `git diff --check`
  - passed
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run`
  - `FINAL RESULTS: 12569/12582 passed (99.9%)`
  - skipped 3, known failures 3, fingerprint-only 9
  - remaining code mismatch: `TypeScript/tests/cases/compiler/symbolLinkDeclarationEmitModuleNamesImportRef.ts` missing TS2883
  - net `12563 -> 12569 (+6)`
