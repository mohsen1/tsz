# fix(checker): align JSDoc chained-assignment fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/jsdoc-chained-assignment-this-display`
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Fix the fingerprint-only conformance mismatch in
`TypeScript/tests/cases/conformance/jsdoc/jsdocTypeFromChainedAssignment.ts`.
Current `main` has matching diagnostic codes (`TS2322`, `TS2339`, `TS2345`) but
reports the wrong surfaces for two chained JSDoc assignment cases:

- `A.s = A.t = function g(m) { return m + this.x }` formats the `this` receiver
  as `g`; tsc reports `typeof A`.
- `A.t('not here either')` is missed while tsz reports an extra `a.z(...)`
  property error.

## Planned Scope

- JSDoc/chained-assignment type propagation in `crates/tsz-checker`.
- A focused regression test for the conformance shape.

## Verification Plan

- `cargo fmt --check`
- Focused checker regression test
- `./scripts/conformance/conformance.sh run --filter "jsdocTypeFromChainedAssignment" --verbose`

## Verification

- `cargo test -p tsz-checker checked_js_chained_assignment_jsdoc_flows_to_all_targets -- --nocapture`
- `./scripts/conformance/conformance.sh run --filter "jsdocTypeFromChainedAssignment" --verbose`
- `cargo fmt --check`
- `git diff --check`
