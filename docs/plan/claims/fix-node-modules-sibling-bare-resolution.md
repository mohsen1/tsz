# fix(resolver): resolve sibling node_modules packages

- Date: 2026-05-02
- Branch: `fix/node-modules-sibling-bare-resolution`
- PR: #2231
- Status: validated locally
- Workstream: 2 (conformance/module resolution)

## Intent

Fix `isolatedModulesReExportType`, where an export inside
`node_modules/baz/index.d.ts` could not resolve bare package `foo` even though
`node_modules/foo/index.d.ts` exists as a sibling package.

## Files Touched

- `crates/tsz-core/src/module_resolver/node_modules_resolution.rs`
- `crates/tsz-core/src/module_resolver/tests.rs`
- `crates/tsz-checker/src/module_resolution.rs`
- `crates/tsz-checker/tests/module_resolution.rs`
- `crates/tsz-cli/src/driver/resolution.rs`
- `crates/tsz-cli/src/driver/resolution_tests.rs`

## Verification

- `cargo nextest run -p tsz-cli test_collect_module_specifiers_finds_re_exports_inside_ambient_module_blocks test_resolve_module_specifier_from_node_modules_package_finds_sibling_package`
- `cargo nextest run -p tsz-checker test_node_modules_sibling_package_bare_alias_is_registered test_scoped_node_modules_package_bare_alias_is_registered`
- `cargo nextest run -p tsz-core test_resolver_bare_specifier_from_node_modules_package_finds_sibling_package`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run --filter isolatedModulesReExportType --verbose`
