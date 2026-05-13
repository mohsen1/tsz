# fix(parser): reject readonly accessor modifier combination

- **Date**: 2026-05-13
- **Branch**: `fix-readonly-accessor-ts1243-20260513`
- **PR**: #6235
- **Status**: ready
- **Workstream**: Diagnostic conformance

## Intent

Close #6188 by emitting TS1243 when a class auto-accessor combines `readonly` with `accessor`, matching TypeScript's syntax-level modifier compatibility rule. Keep the fix in the syntax/diagnostic layer rather than adding checker semantics, because the invalid modifier pair is determined directly from class member modifiers.

## Files Touched

- `docs/plan/claims/fix-readonly-accessor-ts1243-20260513.md`
- `crates/tsz-parser/tests/modifier_ordering_tests.rs`

## Verification

- `cargo test -p tsz-parser ts1243_without_ts1029 -- --nocapture` (2 passed)
- `cargo fmt --all -- --check`
- `git diff --check`
