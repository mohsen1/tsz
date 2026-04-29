# [WIP] fix(checker): report TS2708 for aliased merged module interfaces

- **Date**: 2026-04-29
- **Branch**: `fix/checker-alias-merged-module-interface-ts2708`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Fix the `aliasOnMergedModuleInterface.ts` conformance failure selected by
`scripts/session/quick-pick.sh`, where TSZ misses TS2708 for use of a
namespace-style alias that resolves through a merged module/interface symbol.
The implementation will diagnose the root cause in the checker/binder boundary
without adding a checker-local semantic shortcut.

## Files Touched

- TBD after diagnosis

## Verification

- Planned: `cargo check --package tsz-checker`
- Planned: `cargo check --package tsz-solver`
- Planned: `cargo build --profile dist-fast --bin tsz`
- Planned: owning-crate `cargo nextest run` unit test
- Planned: `./scripts/conformance/conformance.sh run --filter "aliasOnMergedModuleInterface" --verbose`
- Planned: `./scripts/conformance/conformance.sh run --max 200`
- Planned: `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL`
