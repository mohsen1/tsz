# fix(checker): report JSDoc template diagnostics inside callbacks

- **Date**: 2026-04-27
- **Branch**: `fix-jsdoc-template-inside-callback`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the all-missing conformance case `jsdoc/templateInsideCallback.ts`, where TSZ currently reports no diagnostics while TypeScript reports invalid JSDoc template ordering, unresolved template names, non-generic callback use, and implicit-any fallout. The slice should keep the fix narrowly routed through existing JSDoc parser/checker diagnostics and land a regression test for the picked case or the smallest representative pattern.

## Files Touched

- `crates/tsz-checker/src/jsdoc/diagnostics.rs`
- `crates/tsz-checker/src/jsdoc/parsing.rs`
- `crates/tsz-checker/src/jsdoc/resolution/type_construction.rs`
- `crates/tsz-checker/tests/jsdoc_reference_kernel_tests.rs`
- `crates/tsz-cli/src/driver/core.rs`

## Verification

- `cargo fmt --check` (passes)
- `cargo test -p tsz-checker --lib template_inside_callback_reports_invalid_template_and_fallout -- --nocapture` (passes)
- `./scripts/conformance/conformance.sh run --filter "templateInsideCallback" --verbose` (1/1 passed)
- `cargo test -p tsz-cli --lib` (730 passed, 1 failed, 11 ignored): failure is `driver_tests::compile_array_from_iterable_uses_real_lib_iterable_overload`, which also fails on a clean detached worktree at `08e8a1e0fc` with `[2322]` vs expected `[2769]`; not introduced by this branch.
