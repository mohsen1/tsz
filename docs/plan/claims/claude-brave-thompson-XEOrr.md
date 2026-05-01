# jsx: route empty-spread missing-required check through solver assignability

- **Date**: 2026-05-01
- **Branch**: `claude/brave-thompson-XEOrr`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Conformance fixes)

## Intent

Eliminate a JSX-spread false-positive TS2741 by gating the property-by-property
missing-required-prop check on the solver's whole-spread assignability. When
`is_assignable_to(spread_type, props_type)` is true (e.g. `{}` against a target
that requires only `Object.prototype` members like `toString`), the spread
satisfies the props type structurally — including inherited Object members —
and the per-property walk over the *declared* shape would otherwise emit a
false TS2741 because it never sees those inherited members.

Fixes the fingerprint-only divergence for
`tsxAttributeResolution5.tsx` (the `<test2 {...{}} />` line).

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/props/resolution.rs` (~12 LOC)
- `crates/tsz-checker/tests/jsx_spread_assignability_suppresses_ts2741.rs` (new)

## Verification

- New integration test passes (`cargo nextest run -p tsz-checker --test jsx_spread_assignability_suppresses_ts2741`).
- `./scripts/conformance/conformance.sh run --filter "tsxAttributeResolution5" --verbose` passes.
- `scripts/session/verify-all.sh` clean (no regressions).
