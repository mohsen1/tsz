# Claim: widen mutable object literal property typeof queries (#6400)

Status: claim
Owner: Codex
Branch: fix-typeof-property-widen-6400-20260513
Issue: #6400

## Scope

Fix the false TS2322 where `typeof obj.a` from `const obj = { a: 1 }` keeps literal `1` instead of widening to `number`, while preserving `as const` literal behavior.

## Validation target

- focused regression for #6400
- nearby object literal/property access tests if affected
- `cargo fmt --all -- --check`
