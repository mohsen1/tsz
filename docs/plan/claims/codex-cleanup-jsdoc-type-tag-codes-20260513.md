# Claim: dedupe JSDoc type tag diagnostic code projections

Status: Ready
Owner: Codex
Issue: https://github.com/mohsen1/tsz/issues/6257
Branch: `codex/cleanup-jsdoc-type-tag-codes-20260513`

## Scope

- Add a local `diagnostic_codes` helper in `jsdoc_type_tag_tests.rs`.
- Replace repeated diagnostic code-list projections in assertion output.
- Keep this behavior-preserving and limited to the checker integration test.

## Verification Plan

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --test jsdoc_type_tag_tests --no-fail-fast`

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --test jsdoc_type_tag_tests --no-fail-fast` (33 passed)
