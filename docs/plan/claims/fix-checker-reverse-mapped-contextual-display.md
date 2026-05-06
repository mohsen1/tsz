# fix(checker): preserve reverse mapped contextual diagnostic display

- **Date**: 2026-05-06
- **Branch**: `fix/checker-reverse-mapped-contextual-display`
- **PR**: https://github.com/mohsen1/tsz/pull/3464
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

This PR targets the quick-picked fingerprint-only conformance mismatch in
`TypeScript/tests/cases/compiler/reverseMappedTypeContextualTypeNotCircular.ts`.
The diagnostic code set already matches TypeScript (`TS2322`), but the rendered
type fingerprint differs.

## Context

`docs/plan/claims/investigate-diagnostic-type-display-alias-preservation.md`
lists this fixture as part of a broader alias/application display hand-off. This
claim narrows the work to the reverse-mapped contextual display case selected by
`scripts/session/quick-pick.sh --seed 3407`.

## Files Touched

- `crates/tsz-checker/src/error_reporter/assignability.rs`
- `crates/tsz-checker/tests/ts2322_tests.rs`
- `docs/plan/claims/fix-checker-reverse-mapped-contextual-display.md`

## Verification

- `cargo fmt --check`
- `CARGO_TARGET_DIR=.target CARGO_INCREMENTAL=0 cargo nextest run --target-dir .target -p tsz-checker --test ts2322_tests test_reverse_mapped_contextual_target_display_uses_inferred_application_args`
- `CARGO_TARGET_DIR=.target CARGO_INCREMENTAL=0 cargo build --target-dir .target --profile dist-fast -j 4 -p tsz-cli -p tsz-conformance`
- `./.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file /Users/mohsen/code/tsz/scripts/conformance/tsc-cache-full.json --tsz-binary ./.target/dist-fast/tsz --filter 'reverseMappedTypeContextualTypeNotCircular' --verbose --print-fingerprints --write-diff-artifacts --diff-artifacts-dir artifacts/conformance/reverse-mapped-contextual-display --workers 2 --max-worker-rss-mb 1024 --max-compilations-per-worker 10`
- `CARGO_TARGET_DIR=.target CARGO_INCREMENTAL=0 cargo nextest run --target-dir .target -p tsz-checker --test ts2322_tests`
