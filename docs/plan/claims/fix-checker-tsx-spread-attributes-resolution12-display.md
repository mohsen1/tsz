# fix(checker): align TSX spread attributes resolution12 display

- **Date**: 2026-05-06
- **Branch**: `fix/checker-tsx-spread-attributes-resolution12-display`
- **PR**: https://github.com/mohsen1/tsz/pull/3518
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

This PR targets the quick-picked fingerprint-only conformance mismatch in
`TypeScript/tests/cases/conformance/jsx/tsxSpreadAttributesResolution12.tsx`.
The diagnostic code set already matches TypeScript (`TS2322`), but tsz reports
extra per-attribute object displays and misses the merged spread-source display.

## Context

PR #1947 already suppressed one any-spread-related extra diagnostic for this
fixture and left follow-up work on merged spread-source display and anchoring.
This claim narrows the remaining work to the quick-picked `tsxSpreadAttributesResolution12`
fingerprint mismatch selected with `scripts/session/quick-pick.sh --seed 3408`.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/props/resolution.rs`
- `crates/tsz-checker/src/checkers/jsx/props/synthesized_display.rs`
- `crates/tsz-checker/src/checkers/jsx/spread.rs`
- `crates/tsz-checker/src/checkers/jsx/tests.rs`

## Verification

- `cargo fmt --check`
- `CARGO_TARGET_DIR=.target-pr3518 CARGO_INCREMENTAL=0 cargo nextest run --target-dir .target-pr3518 -p tsz-checker jsx_spread_attributes_resolution12_reports_merged_effective_source_once`
- `./.target-pr3518/dist-fast/tsz-conformance --test-dir /tmp/tsz-typescript-050880/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary ./.target-pr3518/dist-fast/tsz --filter 'tsxSpreadAttributesResolution12' --verbose --print-fingerprints --write-diff-artifacts --diff-artifacts-dir artifacts/conformance/tsx-spread-attributes-resolution12 --workers 2 --max-worker-rss-mb 1024 --max-compilations-per-worker 10`
