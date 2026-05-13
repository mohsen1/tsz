# Claim: dedupe JSDoc accessibility diagnostic code projections

Status: Ready
Owner: Codex
Issue: https://github.com/mohsen1/tsz/issues/6266
Branch: `codex/cleanup-jsdoc-accessibility-codes-20260513`

## Scope

- Add a local `diagnostic_codes` helper in `jsdoc_accessibility_tests.rs`.
- Replace repeated diagnostic code-list projections in assertion output.
- Keep this behavior-preserving and limited to the checker integration test.

## Verification Plan

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(jsdoc_accessibility_tests::)' --no-fail-fast`

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(jsdoc_accessibility_tests::)' --no-fail-fast` (6 passed)
