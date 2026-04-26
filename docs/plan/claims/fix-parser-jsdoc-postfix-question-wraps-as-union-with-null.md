# fix(parser): wrap JSDoc-style postfix `T?` as `T | null` UNION_TYPE

- **Date**: 2026-04-26
- **Branch**: `fix/parser-jsdoc-postfix-question-wraps-as-union-with-null`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 — Diagnostic Conformance

## Intent

`jsdocDisallowedInTypescript.ts` emitted `Type 'undefined' is not assignable
to type 'number'.` for `var postfixopt: number? = undefined;` because tsz's
parser consumed the postfix `?` (with TS17019) but didn't change the type.
tsc emits TS17019 *and* still resolves the annotation as `number | null`,
so a subsequent assignment from `undefined` reports against `number | null`.

This PR teaches the parser to synthesize a UNION_TYPE wrapping the base
type and a NullKeyword token when consuming a postfix `?`, matching tsc's
JSDoc-nullable semantics. Also fixes the suggestion text in TS17020
(prefix `?T`): when the inner type now spans `T?` (because postfix-? was
also present), the suggestion should still reference just `T`.

## Files Touched

- `crates/tsz-parser/src/parser/state_types.rs` — synthesize UNION_TYPE for
  postfix `?` (~15 LOC) + trim trailing `?` in TS17020 suggestion (~5 LOC).
- `crates/tsz-checker/tests/jsdoc_postfix_nullable_type_tests.rs` — 3 unit
  tests: target widens to `number | null`, array suffix chains correctly,
  conditional-type `?` ternary not misparsed.
- `crates/tsz-checker/src/lib.rs` — register the new test module.

## Verification

- `cargo nextest run -p tsz-checker --lib jsdoc_postfix_nullable` (3 pass)
- `./scripts/conformance/conformance.sh run --filter jsdocDisallowedInTypescript` — PASS (was fingerprint-only FAIL)
- `./scripts/conformance/conformance.sh run --filter expressionWithJSDocTypeArguments` — PASS (TS17020 suggestion fix)
- Full conformance: 12183 → 12192 (+9), no real regressions
  (one apparent regression on `parserClassDeclaration1.ts` is pre-existing
  on main, snapshot drift)
