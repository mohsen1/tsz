**2026-04-27 00:48:07** — JSX spread-overrides-attribute TS2322 anchor

## Scope
When a JSX spread attribute overrides an EARLIER explicit attribute (TS2783 case)
and the spread's property type doesn't match the expected prop type, tsc anchors
the per-property TS2322 at the EXPLICIT attribute's name (matching the TS2783
anchor) and uses the per-property message ("Type 'X' is not assignable to type
'Y'"), not the whole-type message.

Currently, tsz emits the whole-type message at the JSX tag name. This causes a
fingerprint mismatch on `tsxAttributeResolution3` (and likely related JSX tests).

## Plan
1. In `crates/tsz-checker/src/checkers/jsx/spread.rs`, extend
   `check_spread_property_types` to accept a map of earlier explicit attr names
   to their name node indices.
2. When a per-property mismatch is found whose property name has an earlier
   explicit attr entry, emit a per-property TS2322 at that attr's name node
   instead of (or in addition to) the whole-type TS2322 at the tag name.
3. Update the call site in `crates/tsz-checker/src/checkers/jsx/props/resolution.rs`
   to compute and pass earlier explicit attr names.
4. Ship a unit test in `tsz-checker` and verify
   `TypeScript/tests/cases/conformance/jsx/tsxAttributeResolution3.tsx`
   passes with no regressions.
