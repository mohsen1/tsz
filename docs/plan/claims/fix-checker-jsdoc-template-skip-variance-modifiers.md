# fix(checker): skip `in` / `out` variance modifiers in JSDoc `@template` parsing

- **Date**: 2026-04-29
- **Branch**: `fix/checker-jsdoc-template-skip-variance-modifiers`
- **PR**: #1759
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Fix wrong-code emission on `conformance/jsdoc/jsdocTemplateTag8.ts`: tsz
parsed `@template in T` and `@template out T` as TWO type parameters
(named `in`/`out` and `T`), causing cascading TS2314
("Generic type 'X' requires 2 type argument(s)") and TS7006
("Parameter implicitly has an 'any' type") false positives across the
typedef body.

tsc treats `in` and `out` as type-parameter variance modifiers — like
the existing `const` modifier — not as type-parameter names. They are
only valid on class/interface/type-alias declarations (TS1274 fires
elsewhere; that diagnostic is emitted by a separate validator and is
out of scope here), but the parser must not turn them into spurious
type parameters in any case.

## Root Cause

Two `@template` parsers in the checker (`jsdoc_template_type_params`
in `crates/tsz-checker/src/jsdoc/params.rs` and
`jsdoc_template_constraints` in `crates/tsz-checker/src/jsdoc/parsing.rs`)
already skip `const` but did not skip `in` / `out`. Result: the names
`in` / `out` were pushed into the type-parameter list, the typedef
became "2-arg generic", and downstream consumers emitted TS2314 +
TS7006 cascades.

## Fix

Add `if name == "in" || name == "out" { continue; }` next to the
existing `const` skip in both parsers. Behavior-preserving for all
existing tests; the variance modifier is dropped from parsing and the
type parameter shape matches tsc's.

## Files Touched

- `crates/tsz-checker/src/jsdoc/params.rs` (+11 LOC)
- `crates/tsz-checker/src/jsdoc/parsing.rs` (+6 LOC)

## Verification

- `cargo nextest run -p tsz-checker -E 'test(/jsdoc_template|template_tag/)'` (29/29 pass)
- `./scripts/conformance/conformance.sh run --filter "jsdocTemplateTag8" --verbose`:
  drops 4 false-positive extras (TS2314 ×2 + TS7006 ×2) on the picked test.
- Full conformance: net **12235 → 12241 (+6)**, 8 improvements,
  2 reported regressions (`maxNodeModuleJsDepthDefaultsToZero.ts`,
  `namespaceNotMergedWithFunctionDefaultExport.ts`) need verification —
  both touch JSDoc + namespace merging adjacent to this code path.
