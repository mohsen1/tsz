# [WIP] fix(conformance): suppress isolated metadata import diagnostic

- **Date**: 2026-04-29
- **Branch**: `fix/conformance-quick-pick-20260429-8`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Picked by `scripts/session/quick-pick.sh --run` on 2026-04-29 21:04:08 UTC
after the first picker result was already passing on current `origin/main`.
Target `TypeScript/tests/cases/compiler/emitDecoratorMetadata_isolatedModules.ts`
currently emits an extra `TS2345` where `tsc` expects no diagnostics. This PR
will diagnose the isolated-modules / decorator-metadata import boundary, remove
the false-positive in the owning layer, and add a focused Rust regression test.

## Files Touched

- TBD after implementation.

## Verification

- Planned: `cargo check --package tsz-checker`
- Planned: `cargo check --package tsz-solver`
- Planned: `cargo build --profile dist-fast --bin tsz`
- Planned: owning-crate `cargo nextest run`
- Planned: `./scripts/conformance/conformance.sh run --filter "emitDecoratorMetadata_isolatedModules" --verbose`
- Planned: `./scripts/conformance/conformance.sh run --max 200`
- Planned: `scripts/safe-run.sh ./scripts/conformance/conformance.sh run`
