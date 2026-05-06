# [WIP] fix(checker): suppress false contextual return union TS2322

- **Date**: 2026-05-06
- **Branch**: `fix/contextual-return-union-false-ts2322`
- **PR**: #3483
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

The canonical picker selected
`TypeScript/tests/cases/compiler/inferenceContextualReturnTypeUnion3.ts`, a
false-positive conformance failure where `tsz` emits an extra `TS2322` while
`tsc` emits no diagnostics. I will root-cause the contextual return type /
union inference path, fix the owning layer, and add focused Rust regression
coverage before marking the PR ready.

## Files Touched

- `crates/tsz-checker/src/types/computation/call_inference.rs`
- `crates/tsz-checker/src/types/utilities/core.rs`
- `crates/tsz-checker/tests/contextual_typing_tests.rs`

## Verification

- `cargo check --package tsz-checker`
- `cargo nextest run -j 1 --target-dir /var/tmp/tsz-nextest-3483 -p tsz-checker --test contextual_typing_tests`
- `cargo nextest run --target-dir /var/tmp/tsz-nextest-3483 -p tsz-checker --lib`
- `./scripts/conformance/conformance.sh run --profile dev --filter "inferenceContextualReturnTypeUnion3" --verbose --workers 1`

Note: local `dist-fast` conformance builds were terminated before fixture
execution in this worktree; the targeted fixture passes with the dev-profile
runner.
