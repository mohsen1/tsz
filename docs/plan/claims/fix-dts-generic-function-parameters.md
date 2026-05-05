Status: ready
Branch: fix-dts-generic-function-parameter-leak
Owner: Codex
Started: 2026-05-05

## Intent

Fix declaration emit for `genericFunctionParameters`, where inferred variable
types leaked callback-local type parameter names like `S` into `.d.ts` output.

## Planned Scope

- Type printer handling for non-infer type parameters that are not in an
  emitted type-parameter scope.
- Constraint fallback for leaked type parameters, using `unknown` when there is
  no constraint.
- Focused printer coverage and the TypeScript baseline fixture.

## Verification Plan

- `cargo fmt --package tsz-emitter -- --check`
- `cargo test -p tsz-emitter unscoped_type_parameter_prints_constraint_or_unknown --lib`
- `./scripts/emit/run.sh --dts-only --filter=genericFunctionParameters --verbose --concurrency=1 --timeout=30000`
- `cargo clippy -p tsz-emitter --lib -- -D warnings`
