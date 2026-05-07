# Claim: Fix in-keyword typeguard diagnostics

## Target

`TypeScript/tests/cases/compiler/inKeywordTypeguard.ts`

Snapshot category: wrong-code.

Expected diagnostics include TS2638 and preserve `Record<..., unknown>` display for `in`-created record intersections.

Actual diagnostics currently include mismatched TS2638/TS2322 behavior, an extra TS7053 for unique-symbol element access after `sym in x`, and normalized object-literal display for `in`-created records.

## Plan

Investigate `in`-operator narrowing and index-expression diagnostics around unknown/object operands. Align the checker so the conformance case reports TS2638, avoids the extra TS7053, preserves generic/object narrowing through `typeof "object"`, and keeps tsc-compatible `Record<..., unknown>` diagnostic display without weakening valid element-access errors.

## Verification

Completed:

- focused checker regression:
  `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/in-keyword-typeguard CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 RUSTFLAGS='-Cdebuginfo=0' cargo test -p tsz-checker --test in_chain_narrows_unconstrained_type_param_tests -- --nocapture`
- formatting/whitespace:
  `cargo fmt --all --check && git diff --check`

Pending:

- filtered conformance for `inKeywordTypeguard` after a successful branch binary rebuild. The current machine has concurrent Rust builds in other worktrees, and the optimized branch rebuild was killed before Cargo reported success.
