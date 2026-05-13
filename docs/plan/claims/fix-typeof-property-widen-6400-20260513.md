# Claim: widen mutable object literal property typeof queries (#6400)

Status: ready
Owner: Codex
Branch: fix-typeof-property-widen-6400-20260513
Issue: #6400

## Scope

Fix the false TS2322 where `typeof obj.a` from `const obj = { a: 1 }` keeps literal `1` instead of widening to `number`, while preserving `as const` literal behavior.

## Validation target

- focused regression for #6400
- nearby object literal/property access tests if affected
- `cargo fmt --all -- --check`

## Result

- Fixed `typeof obj.a` for `const obj = { a: 1 }` to widen mutable object-literal property values to primitives.
- Preserved literal property results for `as const` object literals.

## Validation

- `cargo test -p tsz-checker --test ts2322_tests typeof_mutable_object_property_widens_literal_value -- --nocapture`
- `cargo test -p tsz-checker --test ts2322_tests -- --nocapture`
- `cargo fmt --all -- --check`
