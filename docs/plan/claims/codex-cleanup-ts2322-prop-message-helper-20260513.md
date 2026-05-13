# Claim: reuse TS2322 property diagnostic message helper

Status: ready for review
Owner: Codex
Issue: https://github.com/mohsen1/tsz/issues/6197
Branch: `codex/cleanup-ts2322-prop-message-helper-20260513`

## Scope

- Replace the local parse/bind/check diagnostic collection helper in
  `crates/tsz-checker/tests/ts2322_property_decl_annotation_tests.rs` with
  the shared `check_source_code_messages` helper.
- Keep the change behavior-preserving and limited to checker test harness
  cleanup.

## Verification Plan

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --test ts2322_property_decl_annotation_tests --no-fail-fast`

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --test ts2322_property_decl_annotation_tests --no-fail-fast`
