# Claim: conditional-keyof variance diagnostics count hardening

Status: ready
Owner: Codex
Branch: codex/review-audit-followup-20260512
Source PR/comment: #5051 follow-up (missed-review audit)

## Target

Tighten the variance conformance regression so it fails on unexpected extra diagnostics, not only on TS2322-count drift.

## Plan

1. Keep the existing `TS2322 == 8` assertion.
2. Add a total-diagnostics assertion (`diagnostics.len() == 8`) to prevent non-TS2322 regressions from passing silently.

## Result

- Updated test:
  - `conditional_keyof_variance_assignability_matches_tsc`
- File:
  - `crates/tsz-checker/tests/conditional_infer_tests.rs`

Validation:

```text
cargo fmt --all
cargo test -p tsz-checker --test conditional_infer_tests conditional_keyof_variance_assignability_matches_tsc -- --nocapture
```
