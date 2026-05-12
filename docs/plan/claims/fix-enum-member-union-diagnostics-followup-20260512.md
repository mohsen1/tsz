# Claim: enum member union diagnostic follow-up

Status: ready
Owner: Codex
Branch: codex/review-audit-followup-20260512
Source PR/comment: #5640 follow-up (missed-review audit)

## Target

Prevent TS2322 source display from collapsing same-enum member unions back to the parent enum name after enum-member rendering is selected.

## Plan

1. Return the rendered union when enum member names were emitted, even if the `collapsed_enum` fallback path was not used.
2. Add a regression test that fails if `E.A | E.B` regresses back to `E` in TS2322 source display.

## Result

- Updated `format_union_with_collapsed_enum_display` in:
  - `crates/tsz-checker/src/error_reporter/core_formatting.rs`
- Added regression coverage in:
  - `crates/tsz-checker/tests/ts2322_tests.rs`
  - `test_ts2322_same_enum_member_union_source_display_preserved`

Validation:

```text
cargo fmt --all
cargo test -p tsz-checker --test ts2322_tests test_ts2322_same_enum_member_union_source_display_preserved -- --nocapture
cargo test -p tsz-checker --test ts2322_tests enum -- --nocapture
cargo test -p tsz-checker --test recursive_typeof_param_display_tests -- --nocapture
```
