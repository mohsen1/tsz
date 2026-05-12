# fix(checker): emit TS2786 for union JSX components with invalid return types

- **Date**: 2026-05-12
- **Branch**: `claude/stoic-cori-xZnoG`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance and Fingerprints)

## Intent

Fix `jsxComponentTypeErrors.tsx` fingerprint-only TS2786 failure for
`MixedComponent`. When a JSX component type is a union where all members
have extractable props but invalid return types, tsz was silently skipping
the return-type validation and never emitting TS2786. The root cause was
a `continue` in `check_jsx_component_return_type` that treated
"has extractable props" as "valid component" — but having extractable props
says nothing about whether the return type is assignable to `JSX.Element` /
`JSX.ElementClass`.

## Structural Rule

When a union component type has members whose return types are not assignable
to `JSX.Element` (SFC) or `JSX.ElementClass` (class component), tsz must emit
TS2786 for the union — regardless of whether props can be extracted from those
members.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/extraction.rs` (~7 LOC removed, ~2 LOC changed)
- `crates/tsz-checker/src/checkers/jsx/tests.rs` (~50 LOC added)

## Verification

- `cargo test -p tsz-checker --lib "jsx_union_component"` — 2 new tests pass
- `cargo test -p tsz-checker --lib "checkers_domain::jsx"` — 96 tests pass
- `cargo test -p tsz-checker --lib` — 3842 pass, 1 pre-existing failure unrelated to this change
- Conformance test `jsxComponentTypeErrors.tsx` should now emit TS2786 for `MixedComponent`
