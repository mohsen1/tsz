# fix(checker): preserve outer index-signature target in TS2322 missing-property render

- **Date**: 2026-04-26
- **Branch**: `fix/index-sig-target-display-ts2322`
- **PR**: #1422
- **Status**: ready
- **Workstream**: 1 (Conformance — fingerprint-only TS2322 fixes)

## Intent

Fix `numericIndexerConstraint2.ts` (and the wider class of similar tests
where the solver's `MissingProperty` reason describes an inner index-
signature value-type mismatch). The checker's `render_missing_property`
currently mis-classifies these as "primitive source" cases because the
inner `source_type` happens to be a primitive (e.g. `number` from
`{ one: number }`), and then prints the inner value type (e.g. `Foo`) as
the top-level target. tsc instead reports the OUTER source/target — e.g.
`Type '{ one: number; }' is not assignable to type '{ [index: string]: Foo; }'.`.

The fix tightens the `is_source_primitive` predicate so the primitive
shortcut only triggers when the OUTER source itself is primitive (or
displays as a primitive name). The pre-existing depth>0 behavior is
preserved — nested renders still pass through the inner
`source_type`/`target_type` for property-level elaboration.

## Files Touched

- `crates/tsz-checker/src/error_reporter/render_failure.rs` (≈10 LOC change)
- `crates/tsz-checker/tests/class_index_signature_compat_tests.rs`
  (added regression test)

## Verification

- `cargo nextest run -p tsz-checker --lib` (2890/2890 pass)
- `./scripts/conformance/conformance.sh run --filter numericIndexerConstraint2 --verbose`
  flips from fingerprint-only FAIL to PASS
- `./scripts/conformance/conformance.sh run --filter "indexedAccess"` /
  `--filter "indexer"` / `--filter "primitive"` show no regressions vs main
