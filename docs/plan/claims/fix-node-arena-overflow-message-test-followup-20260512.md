# Claim: node arena overflow message regression lock

Status: ready
Owner: Codex
Branch: codex/review-audit-followup-20260512
Source PR/comment: #5095 follow-up (missed-review audit)

## Target

Add a focused regression test to lock the `NodeArena::len_u32` overflow panic message so future changes do not silently regress wording relied on by diagnostics/debug workflows.

## Plan

1. Add a `#[should_panic(expected = ...)]` unit test in `node_arena.rs`.
2. Trigger the overflow path without allocating large memory by calling `len_u32(usize::MAX)`.

## Result

- Added test:
  - `parser::node_arena::tests::len_u32_overflow_panics_with_expected_message`
- File:
  - `crates/tsz-parser/src/parser/node_arena.rs`

Validation:

```text
cargo fmt --all
cargo test -p tsz-parser len_u32_overflow_panics_with_expected_message -- --nocapture
```
