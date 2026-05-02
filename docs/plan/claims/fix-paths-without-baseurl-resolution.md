# fix(resolver): resolve paths mappings without baseUrl

- **Date**: 2026-05-02
- **Branch**: `fix/paths-without-baseurl-resolution`
- **PR**: #2232
- **Status**: ready
- **Workstream**: 1 (Conformance fixes)

## Intent

TypeScript resolves relative `paths` substitutions from the config directory even when `baseUrl` is not set. tsz currently skips `paths` resolution entirely without `baseUrl`, which turns real path-mapped imports into TS2307 and prevents the TS5097 extension diagnostic from matching tsc. This PR makes the resolver use the project/config directory as the mapping base only for `paths` substitutions when `baseUrl` is absent.

## Files Touched

- `crates/tsz-cli/src/driver/resolution.rs`
- `crates/tsz-cli/src/driver/resolution_tests.rs`
- `crates/tsz-checker/src/declarations/import/declaration.rs`
- `crates/tsz-cli/tests/driver_tests.rs`

## Verification

- `cargo nextest run -p tsz-cli test_resolve_module_specifier_paths_without_base_url_use_project_base compile_paths_without_base_url_resolve_before_ts_extension_diagnostic` (2 passed)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run --filter resolutionCandidateFromPackageJsonField2 --verbose` (1/1 passed)
- `cargo nextest run -p tsz-cli` (1077 passed, 15 skipped)
- `cargo nextest run -p tsz-checker` (5902 passed, 37 skipped)
