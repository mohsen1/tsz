# fix(checker): reject parameter symbols in lib globalThis var lookup

- **Date**: 2026-04-30
- **Branch**: `fix-globalthis-const-not-property`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Conformance)

## Intent

`globalThisReadonlyProperties.ts` and `globalThisBlockscopedProperties.ts`
both expect TS2339 ("Property 'y' does not exist on type 'typeof
globalThis'") for `globalThis.y` writes/reads when `y` is a `const`/`let` —
because block-scoped declarations are not properties of `typeof globalThis`.

tsz emitted no TS2339 for these. The diagnostic path in
`resolve_global_this_property_type` was falling through to its "before
erroring, see if a lib `var` exists with this name" fallback. That fallback
called `resolve_lib_global_var_symbol`, which walked `lib_symbol_ids`
filtering by `VALUE` flag plus a "not block-scoped" check — which is
satisfied by the parameter `y` of `Math.atan2(y, x)` from `lib.es5.d.ts`
(parameters carry `FUNCTION_SCOPED_VARIABLE`, same flag as a real `var`).
The spoofed match suppressed the legitimate TS2339.

The fix narrows `resolve_lib_global_var_symbol` to require at least one
declaration whose syntactic kind is plausibly a global value (anything
*except* `Parameter`). The deeper question — why parameter symbols leak
into `lib_symbol_ids` at all — is left for a separate binder-side fix; the
lookup-site filter is the smaller, lower-risk change.

## Files Touched

- `crates/tsz-checker/src/symbols/symbol_resolver_utils.rs` —
  `resolve_lib_global_var_symbol` now also requires
  `symbol_has_globalable_declaration`, a new helper that rejects
  parameter-only symbols (≈30 LOC including doc-comment and helper).
- `crates/tsz-checker/tests/global_this_const_property_tests.rs` — three
  integration tests: TS2339 emitted on `globalThis.y` write/read with
  `const y`, and *not* emitted on `globalThis.x` write with `var x`
  (positive counter-test for the flag filter).
- `crates/tsz-checker/Cargo.toml` — register the new test target.

## Verification

- `cargo nextest run -p tsz-checker --test global_this_const_property_tests` — 3/3 pass.
- `cargo nextest run -p tsz-checker` — 5775 tests pass.
- `bash scripts/conformance/conformance.sh run` — net **+4** (12282 → 12286):
  7 FAIL → PASS (`globalThisReadonlyProperties.ts`,
  `globalThisBlockscopedProperties.ts` directly; 5 indirect siblings),
  3 PASS → FAIL that are stale-baseline artifacts: each fails on
  `origin/main` without this PR as well
  (`contextuallyTypeAsyncFunctionReturnTypeFromUnion.ts`,
  `constAssertions.ts`, `unionTypeInference.ts`).
