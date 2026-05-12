# Claim: generic-union `in` operator assertion made order-agnostic

Status: ready
Owner: Codex
Branch: codex/review-audit-followup-20260512
Source PR/comment: #5096 follow-up (missed-review audit)

## Target

Remove union-order brittleness from the `in`-operator generic-union TS2322 regression assertion.

## Plan

1. Replace exact union string match (`T | { a: string; }`) with semantic substring checks.
2. Keep strict signal that the diagnostic is TS2322 against `object`.

## Result

- Updated test:
  - `in_operator_reports_ts2322_for_generic_union_rhs`
- File:
  - `crates/tsz-checker/tests/in_chain_narrows_unconstrained_type_param_tests.rs`

Validation:

```text
cargo fmt --all
cargo test -p tsz-checker --test in_chain_narrows_unconstrained_type_param_tests in_operator_reports_ts2322_for_generic_union_rhs -- --nocapture
```
