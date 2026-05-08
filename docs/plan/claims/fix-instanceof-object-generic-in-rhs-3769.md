# fix(checker): instanceof Object narrowing permits generic in-operator RHS (#3769)

- **Date**: 2026-05-08
- **Branch**: `fix/instanceof-object-generic-in-rhs-3769`
- **PR**: TBD
- **Status**: claim
- **Workstream**: narrowing parity

## Intent

`x instanceof Object && 'a' in x` on a generic `x` should accept the
`in`-operator RHS — `instanceof Object` proves at runtime that `x` is
non-primitive. tsz emitted a false TS2638 ("may represent a primitive
value") because the instanceof narrowing produced `T & {}`
(`NonNullable<T>`) for type-parameter sources, and `{}` doesn't
exclude primitives in the `in`-RHS validity check.

## Fix

In `narrow_by_instance_type`, before the generic `narrow_type_param`
fallback, special-case the `instanceof Object` shape: when the source
is a bare type parameter or an intersection containing one, return
`source & TypeId::OBJECT` (the `object` keyword) instead of letting
narrowing degrade to `T & {}`. `TypeId::OBJECT` is recognised by
`type_may_represent_primitive` /
`in_operator_intersection_member_excludes_primitive` as
unambiguously non-primitive, so the downstream `in`-RHS check
correctly accepts the narrowed value.

The fix flips the issue's repro and the conformance test
`compiler/inKeywordAndUnknown.ts` (previously a fingerprint-only
failure on the `T & {}` arm) from FAIL to PASS.

## Files Touched

- `crates/tsz-solver/src/narrowing/instanceof.rs` — pre-check before
  the type-parameter narrowing fallback.
- `crates/tsz-checker/src/tests/in_narrow_bare_type_param_chained_tests.rs`
  — `instanceof_object_narrowing_does_not_emit_ts2638_on_generic`
  locks both repro shapes.

## Verification

- New unit test passes.
- `cargo nextest run -p tsz-checker -p tsz-solver` — 12917 / 12917 pass.
- Conformance test `inKeywordAndUnknown` now PASSES (was fingerprint-only).
