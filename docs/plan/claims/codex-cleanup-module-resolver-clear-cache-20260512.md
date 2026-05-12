# chore(core): clear all module resolver caches

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-module-resolver-clear-cache-20260512`
- **PR**: #5678
- **Status**: ready
- **Workstream**: 8.4 (cache hygiene cleanup)

## Intent

Make `ModuleResolver::clear_cache` match its name by clearing every resolver
cache, including the thread-local file-existence cache used by resolution
helpers. This removes a stale dead-code allowance on the existing reset helper
and prevents follow-up resolutions from seeing file paths that were probed
before they existed.

## Files Touched

- `crates/tsz-core/src/module_resolver/mod.rs`
- `crates/tsz-core/src/module_resolver/tests.rs`
- `crates/tsz-core/src/resolution/helpers.rs`
- `docs/plan/claims/codex-cleanup-module-resolver-clear-cache-20260512.md`

## Verification

- `cargo fmt -p tsz-core`
- `cargo test -p tsz-core module_resolver::tests::test_resolver_clear_cache_drops_file_existence_entries -- --exact`
- `cargo check -p tsz-core`
- `cargo test -p tsz-core module_resolver::tests::`
- `cargo clippy -p tsz-core --all-targets -- -D warnings`
- `git diff --check`
