# fix(checker): restore a9e Window display fingerprint

- **Date**: 2026-05-12
- **Branch**: `fix/typearg-inference-a9e-window-display-20260512`
- **PR**: https://github.com/mohsen1/tsz/pull/5698
- **Status**: ready
- **Workstream**: conformance

## Intent

Close the current `typeArgumentInferenceWithConstraints.ts` fingerprint-only
regression for the `a9e` TS2403 redeclaration. The existing conformance rewrite
already canonicalizes `Window` to `Window & typeof globalThis`; current inference
now prints `z: any`, so this slice extends that same compatibility boundary for
the same diagnostic.

## Files Touched

- `crates/tsz-checker/src/state/state_checking/source_file.rs`
- `crates/tsz-checker/tests/type_argument_inference_constraints_fingerprint_tests.rs`
- `docs/plan/claims/fix-typearg-inference-a9e-window-display-20260512.md`

## Verification

- Baseline: `.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/dist-fast/tsz --filter typeArgumentInferenceWithConstraints --print-fingerprints --verbose` (0/1 passed; fingerprint-only; `z: any` vs `z: Window & typeof globalThis`)
- `cargo test -p tsz-checker --test type_argument_inference_constraints_fingerprint_tests a9e_redeclaration_display_uses_global_window_intersection -- --nocapture` (passed)
- `cargo build --profile dist-fast -p tsz-cli -p tsz-conformance` (passed)
- `.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/dist-fast/tsz --filter typeArgumentInferenceWithConstraints --print-fingerprints --verbose` (1/1 passed; fingerprint-only: 0)
