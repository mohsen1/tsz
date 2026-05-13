# Claim: dedupe JSDoc readonly diagnostic code projections

Status: WIP
Owner: Codex
Issue: https://github.com/mohsen1/tsz/issues/6245
Branch: `codex/cleanup-jsdoc-readonly-codes-20260513`

## Scope

- Add a local `diagnostic_codes` helper in `jsdoc_readonly_tests.rs`.
- Replace repeated diagnostic code-list projections in assertion messages and
  local `codes` bindings.
- Keep this behavior-preserving and limited to the checker integration test.

## Verification Plan

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --test jsdoc_readonly_tests --no-fail-fast`
