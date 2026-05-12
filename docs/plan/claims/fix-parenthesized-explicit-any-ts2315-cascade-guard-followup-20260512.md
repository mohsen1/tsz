# Claim: parenthesized explicit-any TS2315 cascade guard

Status: ready
Owner: Codex
Branch: codex/review-audit-followup-20260512
Source PR/comment: #4992 follow-up (missed-review audit)

## Target

Harden the parenthesized explicit-any alias regression test so it also fails if TS2344 cascades reappear.

## Plan

1. Keep existing `TS2315` presence assertion for `type Wrapped = ((any)); type x = Wrapped<1>`.
2. Add explicit `!TS2344` assertion to lock non-cascading behavior.

## Result

- Updated test:
  - `ts2315_fires_on_parenthesized_explicit_any_alias_body`
- File:
  - `crates/tsz-checker/tests/ts2315_explicit_any_type_alias_tests.rs`

Validation:

```text
cargo fmt --all
cargo test -p tsz-checker --test ts2315_explicit_any_type_alias_tests ts2315_fires_on_parenthesized_explicit_any_alias_body -- --nocapture
```
