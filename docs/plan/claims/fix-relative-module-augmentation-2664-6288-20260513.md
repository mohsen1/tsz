# fix(checker): report TS2664 for unresolved relative module augmentations

- **Date**: 2026-05-13
- **Branch**: `fix-relative-module-augmentation-2664-6288-20260513`
- **PR**: #6293
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance / module resolution)

## Intent

Issue #6288 reports that `declare module "./nonexistent"` is accepted even though tsc emits TS2664 for unresolved relative module augmentations. The fix should stay in module-augmentation validation / resolution plumbing and add a focused regression for the relative path case, preserving existing package-name TS2664 behavior.

## Files Touched

- `crates/tsz-checker/src/declarations/declarations_module.rs`
- `crates/tsz-cli/tests/tsc_compat_tests.rs`

## Verification

- `cargo run -p tsz-cli --bin tsz -- --noEmit --strict --pretty false /tmp/issue6288.ts` emitted TS2664 for `./nonexistent` as expected (exit 2 due diagnostics).
- `cargo test -p tsz-cli --test tsc_compat_tests relative_module_augmentation_missing_target_reports_ts2664 -- --nocapture`
- `cargo fmt --all -- --check`
- `git diff --check`
