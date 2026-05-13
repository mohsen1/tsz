# Claim: reuse override intersection diagnostic helper

Status: ready for review
Owner: Codex
Issue: https://github.com/mohsen1/tsz/issues/6216
Branch: `codex/cleanup-override-intersection-helper-20260513`

## Scope

- Replace local custom-option diagnostic projection in
  `crates/tsz-checker/tests/override_intersection_display_tests.rs` with the
  shared `check_with_options_code_messages` helper.
- Keep the `no_implicit_override` option behavior unchanged.
- Keep the change behavior-preserving and limited to checker test harness
  cleanup.

## Verification Plan

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(override_intersection_display_tests::)' --no-fail-fast`

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(override_intersection_display_tests::)' --no-fail-fast`
