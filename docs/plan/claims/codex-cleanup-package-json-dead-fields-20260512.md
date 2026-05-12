# chore(core): remove dead package.json fields

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-package-json-dead-fields-20260512`
- **PR**: #5715
- **Status**: ready
- **Workstream**: 8.4 (dead code cleanup)

## Intent

Remove unread `version` and `module` fields from the core resolver's
`PackageJson` model. `serde` ignores unknown `package.json` fields by default,
so storing these values was unnecessary and only kept a broad
`#[allow(dead_code)]` on the whole struct.

## Files Touched

- `crates/tsz-core/src/resolution/helpers.rs`
- `docs/plan/claims/codex-cleanup-package-json-dead-fields-20260512.md`

## Verification

- `cargo fmt -p tsz-core`
- `cargo test -p tsz-core module_resolver::`
- `cargo clippy -p tsz-core --all-targets -- -D warnings`
- `git diff --check`
