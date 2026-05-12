# chore(resolution): remove unused self-reference result data

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-module-resolver-self-reference-dead-data-20260512`
- **PR**: #5692
- **Status**: ready
- **Workstream**: 8.4 (dead code cleanup)

## Intent

Remove unused payload fields from `SelfReferenceResultV2::AmbiguousRoot`.
Callers only branch on the variant and do not read the export-map entry or
package-json path, so carrying those strings only required a dead-code
allowance.

## Files Touched

- `crates/tsz-core/src/module_resolver/self_reference.rs`
- `crates/tsz-core/src/module_resolver/node_modules_resolution.rs`
- `docs/plan/claims/codex-cleanup-module-resolver-self-reference-dead-data-20260512.md`

## Verification

- `cargo fmt -p tsz-core`
- `cargo check -p tsz-core`
- `cargo clippy -p tsz-core --all-targets -- -D warnings`
- `cargo test -p tsz-core module_resolver::tests::test_self_reference_exports_pattern_with_ts_key_marks_ts_extension_usage -- --exact`
