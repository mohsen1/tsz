# fix(checker): simplify trivial conditional identity

Status: ready

## Claim

`T extends T ? T : never` and aliases that instantiate to that shape are an identity
type, including the distributive `T = never` case. The solver now evaluates that
exact shape to `T`, removing false TS2322 diagnostics when a `T` value is assigned
back into `Extract<T, T>`-style targets.

## Evidence

- `cargo nextest run -p tsz-checker --test conditional_infer_tests`
- `./scripts/conformance/conformance.sh run --filter "conditionalTypesSimplifyWhenTrivial" --verbose`
