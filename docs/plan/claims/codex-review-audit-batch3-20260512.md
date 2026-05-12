# fix(audit): map variadic tuple source-display positions through rest and trailing suffix

- **Date**: 2026-05-12
- **Branch**: `codex/review-audit-batch3-20260512`
- **PR**: #5874
- **Status**: ready
- **Workstream**: review audit follow-ups

## Intent

Close missed high-signal review comments from PR #5067 and PR #5108: tuple source-display contextual mapping used only fixed-slot indexing (`elements.get(position)`), which dropped rest-backed contextual typing for variadic tuples and widened display unexpectedly.

## Files Touched

- `crates/tsz-checker/src/error_reporter/core/diagnostic_source/tuple_source_display.rs`
- `crates/tsz-checker/tests/ts2322_tests.rs`

## Verification

- `cargo test -p tsz-checker variadic_tuple_source_display_maps_middle_positions_to_rest_before_suffix -- --nocapture`
