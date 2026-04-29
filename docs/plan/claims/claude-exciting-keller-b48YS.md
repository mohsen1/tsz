# fix(checker): emit TS2314 for bare Array/Promise in JSDoc @param/@return under noImplicitAny

- **Date**: 2026-04-29
- **Branch**: `claude/exciting-keller-b48YS`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (conformance)

## Intent

tsc emits TS2314 ("Generic type 'X' requires N type argument(s)") for bare
`@param {Array}` and `@return {Promise}` annotations when `noImplicitAny` is
active, because those lib types require a type argument. The checker was
silently treating them as `Array<any>` / `Promise<any>` via
`resolve_jsdoc_implicit_any_builtin_type`, causing `required_generic_count_for_jsdoc_type_name`
to early-return without emitting TS2314.

The fix adds a new pass at the end of `check_jsdoc_typedef_base_types` that
scans every JSDoc comment for bare `@param`/`@return` type names, filters to
only the lib-builtin types that `resolve_jsdoc_implicit_any_builtin_type`
handles (and thus skips in the params path), looks them up in the merged
symbol table to get their type-parameter list, and emits TS2314 when at least
one required type argument is missing. This resolves the
`jsdocArrayObjectPromiseNoImplicitAny.ts` conformance fingerprint.

## Files Touched

- `crates/tsz-checker/src/jsdoc/diagnostics.rs` (~70 LOC added)
- `crates/tsz-checker/Cargo.toml` (new `[[test]]` entry)
- `crates/tsz-checker/tests/jsdoc_param_return_ts2314_tests.rs` (new, ~230 LOC)

## Verification

- `cargo nextest run -p tsz-checker --lib` — 2964/2964 pass
- `cargo test -p tsz-checker --test jsdoc_param_return_ts2314_tests` — 6/6 pass
- `./scripts/conformance/conformance.sh run --filter "jsdocArrayObjectPromiseNoImplicitAny"` — 1/1 pass
