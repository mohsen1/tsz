# Claim: recursive Promise self-return avoids false TS2322

Issue: #6623
Branch: codex/recursive-promise-self-6623-20260513
Status: ready

## Summary

Recursive Promise chains already emit TS1062 for the invalid self-referencing fulfillment path. The checker also emitted a follow-on TS2322 with identical source and target display (`Type 'T' is not assignable to type 'T'`) for the recursive async return. This claim removes that false self-display cascade while preserving the TS1062 diagnostic.

## Files changed

- `crates/tsz-checker/src/state/state_checking/source_file.rs`
- `crates/tsz-checker/tests/conditional_infer_tests.rs`
- `docs/plan/claims/codex-recursive-promise-self-6623-20260513.md`

## Validation

- `cargo test -p tsz-checker --test conditional_infer_tests recursive_promise_chain_keeps_only_ts1062_without_self_assignment_ts2322 -- --nocapture`
- `cargo test -p tsz-checker --test conditional_infer_tests -- --nocapture`
- `cargo fmt --all --check`
