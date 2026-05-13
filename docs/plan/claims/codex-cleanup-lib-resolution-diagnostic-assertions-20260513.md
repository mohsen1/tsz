# Claim: lib-resolution diagnostic assertion cleanup

Status: Complete
Owner: Codex
Issue: https://github.com/mohsen1/tsz/issues/6444
Branch: codex/cleanup-lib-resolution-diagnostic-assertions-20260513

## Scope

Clean up repeated diagnostic assertion boilerplate in
`crates/tsz-checker/tests/lib_resolution_identity_tests.rs`.

This is a behavior-preserving checker-test refactor. The intended cleanup is to
centralize repeated diagnostic filtering and error-code assertions used by the
lib-resolution identity cases while preserving the existing assertions.

## Verification Plan

- `cargo fmt --all -- --check`
- `cargo nextest run -p tsz-checker --test lib_resolution_identity_tests --no-fail-fast`
- `cargo clippy --profile ci-lint -p tsz-checker --all-targets -- -D warnings`
- `git diff --check`

Full PR CI must remain green before merge, including lint, unit, dist, wasm,
emit, conformance, and fourslash.

## Local Verification

- `cargo nextest run -p tsz-checker --test lib_resolution_identity_tests --no-fail-fast` (160 passed, 1 skipped)
- `cargo fmt --all -- --check`
- `cargo clippy --profile ci-lint -p tsz-checker --all-targets -- -D warnings`
- `git diff --check`
