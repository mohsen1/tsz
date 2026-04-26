# fix(checker): widen literal arg display when union callable's `| undefined` is synthetic optional

- **Date**: 2026-04-26
- **Branch**: `fix/union-type-call-signatures-fingerprint`
- **PR**: #1459
- **Status**: ready
- **Workstream**: Conformance fingerprint parity (Tier 1 type-display-parity)

## Intent

When TS2345 fires for an argument against a union of callables whose
optional parameter (`b?: T`) contributes the synthetic `| undefined` to
the unioned parameter type, tsc widens the argument display (e.g.
`'string'` instead of `'"hello"'`) and strips the synthetic
`| undefined` from the parameter display (e.g. `'number'` instead of
`'number | undefined'`). Today tsz preserves the literal text and the
synthetic union — producing a fingerprint mismatch on
`unionTypeCallSignatures.ts` and similar tests. This PR extends the
existing optional-non-rest predicate to walk union members and gates the
literal-sensitive argument branch on whether the union is purely a
synthetic optional `| undefined`.

## Files Touched

- `crates/tsz-checker/src/error_reporter/call_errors/display_formatting.rs`
  (~50 LOC change)
- `crates/tsz-checker/tests/optional_param_display_tests.rs`
  (+72 LOC: two regression tests)

## Verification

- `cargo nextest run -p tsz-checker --lib -E "test(union_callable)"`
  (2 new regression tests pass)
- `cargo nextest run -p tsz-checker --lib`
  (2915/2916 pass; the one failure is a pre-existing LOC-limit test for
  `error_reporter/core/diagnostic_source.rs`, untouched by this PR)
- `./scripts/conformance/conformance.sh run --filter "unionTypeCallSignatures.ts"`
  removes 2 extra-fingerprint TS2345 lines (`'"hello"' → 'number | undefined'`)
  and resolves 2 of 3 missing-fingerprint TS2345 lines (lines 36, 48 of the
  test). The remaining missing (line 27) is a separate root cause —
  unequal-signature-count union calls do not emit TS2345 — out of scope
  for this fix.
- `./scripts/conformance/conformance.sh run --filter "optionalFunctionArg"`
  (1/1 still pass — no regression on existing optional-param display)
- `./scripts/conformance/conformance.sh run --filter "callSignature"`
  (40/40 still pass)
- `./scripts/conformance/conformance.sh run --filter "assignmentCompat"`
  (120/128 — same as on `main`, no regression)
