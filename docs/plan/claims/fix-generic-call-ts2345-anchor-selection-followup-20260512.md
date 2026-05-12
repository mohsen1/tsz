# Claim: TS2345 anchor selection hardening in generic call inference test

Status: ready
Owner: Codex
Branch: codex/review-audit-followup-20260512
Source PR/comment: #4994 follow-up (missed-review audit)

## Target

Remove ordering brittleness in the `max2(1, 2)` TS2345 anchor regression test by selecting diagnostics via the expected argument span instead of first-match-by-code.

## Plan

1. Compute the expected first-argument start offset.
2. Filter TS2345 diagnostics by both code and that exact start offset.
3. Assert exactly one matching diagnostic and keep existing message/length checks.

## Result

- Updated test:
  - `self_referential_constraint_fallback_anchors_first_argument_after_contextual_assignment`
- File:
  - `crates/tsz-checker/tests/generic_call_inference_tests.rs`

Validation:

```text
cargo fmt --all
cargo test -p tsz-checker --test generic_call_inference_tests self_referential_constraint_fallback_anchors_first_argument_after_contextual_assignment -- --nocapture
```
