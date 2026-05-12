# fix-symbol-for-comparison-ts2367-20260512

Status: claim
Owner: Codex
Branch: fix-symbol-for-comparison-ts2367-20260512
Issue: #5834

## Scope

Match tsc by emitting TS2367 when comparing distinct `Symbol.for(...)` results that tsz currently treats as overlapping.

## Plan

- Add a focused TS2367 regression for the issue repro.
- Inspect unique-symbol inference/comparison overlap for `Symbol.for(...)` calls.
- Prefer fixing unique-symbol identity/comparability at the type level over a call-site special case.

## Checkpoint - 2026-05-12

Status: ready

Implemented global `const x = Symbol.for(...)` unique-symbol inference across the variable declaration inference paths used by comparison checking and type-alias variable aliasing. Added regression coverage for TS2367 on distinct global `Symbol.for` const bindings and for locally shadowed `Symbol.for` staying plain symbol-overlapping.

Validation:

- `cargo fmt --all`
- `touch crates/tsz-checker/src/types/computation/binary.rs && CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo test -p tsz-checker --lib symbol_for_const_results -- --nocapture` -> 2 passed
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo test -p tsz-checker --lib shadowed_symbol_call -- --nocapture` -> 2 passed

Notes:

- The `touch` was needed because the path-included `binary_tests.rs` module was served from a stale test binary until its parent module mtime changed.
