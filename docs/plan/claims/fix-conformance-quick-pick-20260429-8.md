# [WIP] fix(conformance): suppress isolated metadata import diagnostic

- **Date**: 2026-04-29
- **Branch**: `fix/conformance-quick-pick-20260429-8`
- **PR**: #1825
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Picked by `scripts/session/quick-pick.sh --run` on 2026-04-29 21:04:08 UTC
after the first picker result was already passing on current `origin/main`.
Target `TypeScript/tests/cases/compiler/emitDecoratorMetadata_isolatedModules.ts`
currently emits an extra `TS2345` where `tsc` expects no diagnostics. This PR
will diagnose the isolated-modules / decorator-metadata import boundary, remove
the false-positive in the owning layer, and add a focused Rust regression test.

## Files Touched

- `crates/tsz-checker/src/types/computation/call_helpers.rs`
- `crates/tsz-checker/tests/ts2304_tests.rs`
- `docs/plan/claims/fix-conformance-quick-pick-20260429-8.md`

## Verification

- `cargo fmt --check`
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run -p tsz-checker --test ts2304_tests local_ambient_value_shadows_dom_interface_in_value_position` (1 passed)
- `cargo nextest run --package tsz-checker --lib` (3023 passed, 10 skipped)
- `cargo nextest run --package tsz-solver --lib` (5554 passed, 9 skipped)
- `./scripts/conformance/conformance.sh run --filter "emitDecoratorMetadata_isolatedModules" --verbose` (1/1 passed)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run` (12270/12582 passed)
