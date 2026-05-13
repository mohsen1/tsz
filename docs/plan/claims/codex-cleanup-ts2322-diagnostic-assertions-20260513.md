# Claim: TS2322 diagnostic assertion cleanup

Status: WIP
Owner: Codex
Issue: https://github.com/mohsen1/tsz/issues/6434
Branch: codex/cleanup-ts2322-diagnostic-assertions-20260513

## Scope

Clean up repeated diagnostic assertion boilerplate in
`crates/tsz-checker/tests/ts2322_tests.rs`.

This is a behavior-preserving checker-test refactor. The intended cleanup is to
centralize repeated TS2322 diagnostic-code counts, presence checks, filtered
diagnostic vectors, and code/message projection boilerplate behind local
helpers while preserving the existing assertions.

## Verification Plan

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --test ts2322_tests --no-fail-fast`
- `cargo clippy --profile ci-lint -p tsz-checker --all-targets -- -D warnings`

Full PR CI must remain green before merge, including lint, unit, dist, wasm,
emit, conformance, and fourslash.
