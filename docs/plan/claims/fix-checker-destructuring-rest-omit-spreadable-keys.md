# fix(checker): include non-spreadable keys in destructuring rest `Omit<T, K>`

- **Date**: 2026-04-29
- **Branch**: `fix/checker-destructuring-rest-omit-spreadable-keys`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance and Fingerprints)

## Intent

`destructuringUnspreadableIntoRest.ts` is a fingerprint-only failure where
the destructuring rest type is displayed differently between tsc and tsz.

For a generic `<T extends A>` source, `const { ...rest } = x` should produce
`Omit<T, "method" | "getter" | "setter">` — including the names of public
prototype members (methods, getters, setters) that are NOT spreadable per
tsc's `isSpreadablePropertyOfClass()`. tsz currently builds `Omit<T, K>`
with K only containing the explicitly destructured property names, missing
the non-spreadable ones — so `Omit<T>` becomes `T` (no Omit at all) when
no explicit destructured props are present, and `Omit<T, "publicProp">`
instead of `Omit<T, "method" | "getter" | "setter" | "publicProp">` when
publicProp is destructured.

This PR extends the type-parameter branch in
`compute_object_rest_type` (`crates/tsz-checker/src/state/variable_checking/binding_rest.rs`)
to combine the explicit destructured names with the constraint's
non-spreadable property names (public, on-prototype) before constructing
the `Omit<T, K>` application.

## Out of Scope

- The `this`-typed source case (`const { ...rest } = this` in a class
  method context) needs analogous treatment: `Omit<this, K>`. That requires
  detecting `ThisType` and resolving the enclosing class to enumerate
  prototype members. Deferred to a follow-up.

## Files Touched

- `crates/tsz-checker/src/state/variable_checking/binding_rest.rs` — extend
  the type-parameter branch in `compute_object_rest_type` and add a
  `collect_unspreadable_prototype_names_from` helper (~30-50 LOC).
- `crates/tsz-checker/tests/destructuring_rest_omit_unspreadable_tests.rs`
  — new unit-test lock for the Omit<T, K> construction.

## Verification

- `cargo nextest run -p tsz-checker -- destructuring_rest_omit_unspreadable`
- `./scripts/conformance/conformance.sh run --filter "destructuringUnspreadableIntoRest" --verbose`
  — expects partial improvement: T-extends-A cases (lines 60-89) move from
  `Omit<T, "publicProp">` / `T` to `Omit<T, "method"|"getter"|"setter"|<rest>>`.
  `this`-typed cases (lines 22-47) remain fingerprint-only on the deferred
  `this` work above.
- `scripts/session/verify-all.sh` (when relevant)
