# Claim: Fix JSDoc unresolved-name diagnostics for compound types

- **Branch**: `claude/nice-darwin-tEtrf`
- **Status**: claim
- **Date**: 2026-05-06 05:35:00
- **Issue**: #3408

## Scope

Closes #3408. JSDoc `@param` arrow return types and `@type` object property value
types currently swallow unresolved simple names — `() => Missing` and
`{ a: Missing }` both produce zero diagnostics, while tsc emits TS2304.

The simple-name path (`@param {Missing} x`) already emits TS2304 in the current
tree, so the fix is scoped to the compound-type cases.

## Approach

Add a recursive validator for inner leaves of a JSDoc type expression that
walks object-literal property value types and arrow-function return/param
types and emits TS2304 for unresolved simple-identifier leaves. Wire it into
the `@param`/`@returns`/`@type` paths in `tsz-checker/src/jsdoc/diagnostics.rs`
after the existing top-level simple-name check.

## Verification

- `./scripts/conformance/conformance.sh run --filter "<jsdoc filter>" --verbose`
- `cargo nextest run -p tsz-checker --lib`
