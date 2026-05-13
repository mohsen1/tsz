# fix(checker): report TS1315 for export as namespace in source files

- **Date**: 2026-05-13
- **Branch**: `codex/export-as-namespace-ts1315-6609-20260513`
- **PR**: #6621
- **Status**: ready
- **Workstream**: conformance / checker diagnostic

## Intent

Fix #6609 so `export as namespace` in a regular TypeScript source file emits
TS1315, while declaration files continue to accept the UMD global export form.

## Files Touched

- `crates/tsz-checker/src/state/state_checking/source_file.rs`
- `crates/tsz-checker/tests/ts1203_node_esm_tests.rs`
- `docs/plan/claims/codex-export-as-namespace-ts1315-6609-20260513.md`

## Verification

- `cargo test -p tsz-checker --test ts1203_node_esm_tests export_as_namespace_ -- --nocapture` (2 passed)
- `cargo fmt --all --check`
- `cargo test -p tsz-checker --test ts1203_node_esm_tests -- --nocapture` (14 passed)
