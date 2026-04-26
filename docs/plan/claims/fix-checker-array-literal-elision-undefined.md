# fix(checker): type elided array literal slots as Required undefined

- **Date**: 2026-04-26
- **Branch**: `fix/conformance-target`
- **PR**: TBD
- **Status**: claim
- **Workstream**: Conformance — array literal element typing parity

## Intent

Match tsc's treatment of `OmittedExpression` in non-destructuring array
literals. tsc types a hole (e.g. `[42, , true]`) as
`undefinedWideningType` and pushes it as a `Required` tuple slot, so the
resulting source type is `[number, undefined, true]`. Required source
slots of type `undefined` then satisfy optional target tuple slots like
`string?` because the tuple subtype rule widens optional target slots to
`T | undefined` for required source elements.

Previously tsz dropped the elided slot entirely, which both shifted the
positions of subsequent elements and lost the `undefined` value type.
This caused `optionalTupleElements1.ts` to emit four extra TS2322
diagnostics on `t3 = [42,,true]`, `t4 = [42,,true]`, `t4 = [,"hello",
true]`, and `t4 = [,,true]`.

## Files Touched

- `crates/tsz-checker/src/types/computation/array_literal.rs`
  (~50 LOC: handle `elem_idx.is_none()` to push `undefined` Required tuple
  element / array element; switch tuple `elem_count` to source-position
  length so contextual extraction stays aligned across elisions; fix
  excess-property-check loop to use the same source-aligned index)

## Verification

- `cargo nextest run -p tsz-checker --lib elided_array_literal` — two new
  unit tests pass.
- `cargo nextest run -p tsz-checker --lib` — 2907/2908 pass; the only
  failure (`checker_files_stay_under_loc_limit` on
  `error_reporter/core/diagnostic_source.rs`) is pre-existing on `main`
  and unrelated to this change.
- `cargo nextest run -p tsz-solver --lib` — 5518/5519 pass; the only
  failure (`solver_file_size_ceiling_tests::test_parser_file_size_ceiling`)
  is pre-existing on `main`.
- `./scripts/conformance/conformance.sh run --filter "optionalTupleElements1"`
  → 1/1 passed (was 0/1 fingerprint-only on `main`).
- `./scripts/conformance/conformance.sh run --filter "tuple"`
  → 31/37 passed (baseline had 30/37; net +1 with no regressions).
- `./scripts/conformance/conformance.sh run --filter "arrayLiteral"`
  → 19/19 passed (no regressions).
