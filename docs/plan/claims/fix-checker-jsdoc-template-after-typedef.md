# fix(checker): bind `@template` tags placed after `@typedef` to the typedef host

- **Date**: 2026-04-28
- **Branch**: `fix/checker-jsdoc-template-after-typedef`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance — JSDoc typedef + template parsing)

## Intent

`jsdoc_template_constraints_before_typedef_host` collected only the `@template` tags appearing **before** the first `@typedef`/`@callback`/`@overload` host in a JSDoc comment. tsc accepts `@template` in any position relative to `@typedef`; when a single typedef-style host carries `@template` after the typedef tag, tsz silently dropped the type parameters and emitted a false TS2315 ("Type 'X' is not generic.") at any reference to the typedef.

Fix: when the JSDoc carries exactly one typedef-style host, return the full set of `@template` tags from anywhere in the JSDoc; multi-host JSDocs keep the conservative pre-host slice so templates don't bleed across distinct typedefs.

Surfaced by `compiler/contravariantOnlyInferenceFromAnnotatedFunctionJs.ts` (extra TS2315 with empty expected). Likely also resolves several jsDeclarations* / jsdoc* fingerprint-only failures involving JSDoc typedefs whose `@template`s land after the typedef tag.

## Files Touched

- `crates/tsz-checker/src/jsdoc/parsing.rs`:
  - `jsdoc_template_constraints_before_typedef_host`: switch to `jsdoc_template_constraints` (whole-JSDoc) when there is exactly one host; keep pre-host slice otherwise.
  - 3 new unit tests under `mod tests` exercising single-host (templates after), constrained `@template`, and multi-host fallback.

## Verification

- `cargo nextest run -p tsz-checker --lib -E 'test(parse_jsdoc_typedefs)'` — 3/3 pass.
- `./scripts/conformance/conformance.sh run --filter "contravariantOnlyInferenceFromAnnotatedFunctionJs"` — moves from FAIL (1 extra TS2315) to PASS.
- Targeted runs on `jsdocTemplate` and `typedef` filters confirm no new FAIL→PASS regressions; pre-existing failures are unrelated to this change.
