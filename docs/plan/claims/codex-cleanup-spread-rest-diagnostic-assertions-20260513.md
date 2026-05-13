# Claim: spread/rest diagnostic assertion cleanup

Status: Ready for review
Owner: Codex
Issue: https://github.com/mohsen1/tsz/issues/6423
Branch: codex/cleanup-spread-rest-diagnostic-assertions-20260513

## Scope

Clean up repeated diagnostic assertion boilerplate in
`crates/tsz-checker/tests/spread_rest_tests.rs`.

This is a behavior-preserving checker-test refactor. The intended cleanup is to
centralize repeated diagnostic-code count, presence, filtering, and message
predicate scans behind local helpers while preserving the existing assertions.

## Verification Plan

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --test spread_rest_tests --no-fail-fast`
- `cargo clippy --profile ci-lint -p tsz-checker --all-targets -- -D warnings`

Full PR CI must remain green before merge, including lint, unit, dist, wasm,
emit, conformance, and fourslash.

## Local Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --test spread_rest_tests --no-fail-fast`
  (83 tests passed)
- `cargo clippy --profile ci-lint -p tsz-checker --all-targets -- -D warnings`
