# fix(checker): allow ConstructorParameters rest spread

- **Date**: 2026-05-13
- **Branch**: `codex/constructor-params-spread-6627-20260513`
- **PR**: #6629
- **Status**: ready
- **Workstream**: conformance / solver spread iterability

## Intent

Cover #6627 so `ConstructorParameters<T>` used as a rest parameter can be
spread into `new ctor(...args)` without a false TS2488.

Current `main` already accepts the repro after recent inference/spread fixes, so
this PR adds focused regression coverage only.

## Files Touched

- `crates/tsz-checker/src/tests/call_architecture_tests.rs`
- `docs/plan/claims/codex-constructor-params-spread-6627-20260513.md`

## Verification

- `cargo test -p tsz-checker constructor_parameters_rest_spread_is_iterable --lib -- --nocapture` (1 passed)
- `cargo fmt --all --check`
