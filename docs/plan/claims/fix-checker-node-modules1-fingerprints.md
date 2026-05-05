# fix(checker): align nodeModules1 diagnostic fingerprints

- **Date**: 2026-05-05 19:53:59 UTC
- **Branch**: `fix/checker-node-modules1-fingerprints`
- **PR**: #3266
- **Status**: ready
- **Workstream**: 1 (Conformance - diagnostic fingerprints)

## Intent

The canonical conformance picker selected
`TypeScript/tests/cases/conformance/node/nodeModules1.ts`, currently a
fingerprint-only failure: `tsz` emits the same diagnostic code set as `tsc`
(`TS1471`, `TS1479`, `TS2307`, `TS2834`, `TS2835`) but differs in one or more
diagnostic fingerprints.

This PR fixes the root cause in Node16/NodeNext request-sensitive module
resolution: dynamic import, static import, and require-like requests for the
same specifier must not share one cached target/error, and require-producing
requests must choose extension priority from the target package scope. That
aligns the TS1471/TS1479/TS2834/TS2835 fingerprints in `nodeModules1.ts`.

## Files Touched

- `crates/tsz-core/src/module_resolver/*`
- `crates/tsz-cli/src/driver/*`
- `crates/tsz-checker/src/context/*`
- `crates/tsz-checker/src/declarations/import/*`
- `crates/tsz-checker/src/declarations/dynamic_import_checker.rs`
- focused resolver/checker regression tests

## Verification

- `cargo fmt --all -- --check`
- `CARGO_TARGET_DIR=.target-run cargo check --package tsz-core --package tsz-cli --package tsz-checker`
- `CARGO_TARGET_DIR=.target-run cargo nextest run -p tsz-core --lib test_node16_cjs_require_uses_target_package_scope_extension_priority test_node16_cache_separates --no-fail-fast`
- `CARGO_TARGET_DIR=.target-run cargo nextest run -p tsz-checker --lib resolution_request_errors_do_not_leak_between_import_kinds --no-fail-fast`
- `./scripts/conformance/conformance.sh run --filter nodeModules1 --verbose` (`1/1 passed`, no fingerprint-only failures)
- `./scripts/conformance/conformance.sh run --max 200` (`200/200 passed`)
