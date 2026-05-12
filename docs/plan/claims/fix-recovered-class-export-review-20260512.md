# fix(emit): clear recovered class export review nits

- **Date**: 2026-05-12
- **Branch**: `fix/recovered-class-export-review-20260512`
- **PR**: #5732
- **Status**: ready
- **Workstream**: 2 (Emit pass rate)

## Intent

Follow up on #5729 review comments that landed after the PR was merged. This
skips pending CommonJS class export state for erased `export declare class`
declarations, makes the recovered anonymous named class export regression
assert the generated binding relationship instead of a fixed suffix, and
removes a duplicate JSDoc helper that broke the merge-head CI build.

## Files Touched

- `crates/tsz-emitter/src/emitter/module_emission/exports.rs`
- `crates/tsz-emitter/src/emitter/module_emission/core/tests.rs`
- `crates/tsz-emitter/src/declaration_emitter/helpers/jsdoc.rs`
- `crates/tsz-emitter/src/declaration_emitter/helpers/jsdoc_function_signature.rs`
- `docs/plan/claims/fix-recovered-class-export-review-20260512.md`

## Verification

- `cargo fmt --check -p tsz-emitter`
- `cargo check -p tsz-emitter`
- `cargo nextest run -p tsz-solver test_emitter_file_size_ceiling`
- `cargo nextest run -p tsz-emitter anonymous_named_class commonjs_declare_class`
- `cargo clippy -p tsz-emitter --all-targets -- -D warnings`
- pre-commit direct `tsz-emitter` test scope: 2283 passed, 9 skipped
