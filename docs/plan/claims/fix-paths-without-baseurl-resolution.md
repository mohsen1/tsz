# fix(resolver): resolve paths mappings without baseUrl

- **Date**: 2026-05-02
- **Branch**: `fix/paths-without-baseurl-resolution`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance fixes)

## Intent

TypeScript resolves relative `paths` substitutions from the config directory even when `baseUrl` is not set. tsz currently skips `paths` resolution entirely without `baseUrl`, which turns real path-mapped imports into TS2307 and prevents the TS5097 extension diagnostic from matching tsc. This PR makes the resolver use the project/config directory as the mapping base only for `paths` substitutions when `baseUrl` is absent.

## Files Touched

- `crates/tsz-cli/src/driver/resolution.rs`
- `crates/tsz-cli/src/driver/resolution_tests.rs`
- `crates/tsz-core/src/module_resolver/mod.rs`
- `crates/tsz-core/src/module_resolver/tests.rs`

## Verification

- Planned: `cargo nextest run -p tsz-cli <targeted resolution tests>`
- Planned: `cargo nextest run -p tsz-core <targeted module_resolver tests>`
- Planned: `scripts/safe-run.sh ./scripts/conformance/conformance.sh run --filter resolutionCandidateFromPackageJsonField2 --verbose`
