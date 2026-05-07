# fix(checker): emit globalThis implicit-any property diagnostics

- **Date**: 2026-05-07
- **Branch**: `fix/conformance-next-20260507-044234`
- **PR**: https://github.com/mohsen1/tsz/pull/4319
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

After refreshing the stale conformance snapshot, the canonical picker selected
`TypeScript/tests/cases/conformance/es2019/globalThisUnknownNoImplicitAny.ts`.
The current run is all-missing: `tsc` emits `TS2339`, `TS7015`, `TS7017`,
and `TS7053`, while `tsz` emits no diagnostics.

This slice will root-cause the missing global object property and element
access diagnostics under `noImplicitAny`, preserving the checker/solver
boundary ownership described in the architecture docs.

## Planned Scope

- Treat `Window & typeof globalThis` annotations as the global object boundary
  without forcing the full `Window` surface during annotation resolution.
- Preserve the `win.hi`, `win['hi']`, `this.hi`, `globalThis.hi`,
  `this['hi']`, and `globalThis['hi']` diagnostic split under
  `noImplicitAny`.
- Keep declared `window` accesses from being mistaken for unknown global
  property fallbacks.
- Add focused checker and parser regressions.

## Verification Plan

- `cargo fmt`
- `cargo nextest run -p tsz-checker --test global_this_property_access_diagnostics_tests`
- `cargo nextest run -p tsz-parser variable_annotation_with_window_and_typeof_globalthis_keeps_following_statements`
- `./scripts/conformance/conformance.sh run --filter "globalThisUnknownNoImplicitAny" --verbose`
- `./scripts/conformance/conformance.sh run --filter "topLevelLambda3" --verbose`
- `./scripts/conformance/conformance.sh run --filter "globalThisCapture" --verbose`
- `./scripts/conformance/conformance.sh snapshot --force`
- Pending: pre-commit hook before final push

## Result

The target conformance test now passes. The refreshed snapshot improved from
12,445 to 12,446 passing tests after fixing the `this.window` spillover cases.
