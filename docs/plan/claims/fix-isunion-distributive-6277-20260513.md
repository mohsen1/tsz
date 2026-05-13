# Fix IsUnion distributive conditional evaluation (#6277)

Status: claim
PR: TBD

## Scope

Investigate and fix false TS2322 diagnostics for the standard `IsUnion<T, U = T>` distributive conditional pattern, where instantiated defaults leak as unresolved type parameters and conditionals remain unevaluated.

## Assumptions

- #6310 and #6340 touch mapped-type `as` clause evaluation and overlap an existing mapped-as claim, so this slice avoids that shared surface.
- This slice focuses on the `IsUnion` distributive conditional/default type-argument path and will avoid broad conditional-type rewrites unless required.

## Verification plan

- Reproduce #6277 with a focused CLI case.
- Add focused regression coverage matching tsc output.
- Run the targeted regression and formatting check.
