# Claim: dedupe JSDoc callback rest diagnostic code projections

Status: WIP
Owner: Codex
Issue: https://github.com/mohsen1/tsz/issues/6278
Branch: `codex/cleanup-jsdoc-callback-rest-codes-20260513`

## Scope

- Add a local `diagnostic_codes` helper in `jsdoc_callback_rest_tests.rs`.
- Replace repeated diagnostic code-list projections in assertion output.
- Keep this behavior-preserving and limited to the checker integration test.

## Verification Plan

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(jsdoc_callback_rest_tests::)' --no-fail-fast`
