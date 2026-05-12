# Claim: JSX TS2786 mixed-component anchor assertion

Status: ready
Owner: Codex
Branch: codex/review-audit-followup-20260512
Source PR/comment: #5660 follow-up (missed-review audit)

## Target

Ensure the `MixedComponent` TS2786 regression test validates anchor position, not only message text.

## Plan

1. Capture the JSX source snippet in a local variable.
2. Compute expected anchor offset at `<MixedComponent`.
3. Assert emitted TS2786 `start` matches that location.

## Result

- Updated test:
  - `jsx_union_of_invalid_function_and_class_component_emits_ts2786`
- File:
  - `crates/tsz-checker/src/checkers/jsx/tests.rs`

Validation:

```text
cargo fmt --all
cargo test -p tsz-checker jsx_union_of_invalid_function_and_class_component_emits_ts2786 -- --nocapture
```
