# Claim: mapped-type conformance anchor span safety

Status: ready
Owner: Codex
Branch: codex/review-audit-followup-20260512
Source PR/comment: #4988 follow-up (missed-review audit)

## Target

Harden mapped-type conformance anchor extraction so invalid diagnostic spans fail with clear context instead of unchecked slice panics.

## Plan

1. Ensure diagnostics consumed by this helper are from `test.ts`.
2. Replace direct string indexing with `source.get(start..end)` and explicit panic context on failure.

## Result

- Updated helper:
  - `diagnostic_anchor_text`
- File:
  - `crates/tsz-checker/tests/mapped_type_errors_conformance_tests.rs`

Validation:

```text
cargo fmt --all
cargo test -p tsz-checker --test mapped_type_errors_conformance_tests -- --nocapture
```
