# fix(checker): allow literal narrowing for keyof Lazy(LibType) and IndexAccess(Lazy(LibType), key)

- **Date**: 2026-04-26
- **Branch**: `fix/contextual-literal-indexed-access-lib`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance — false positive on `arrayToLocaleStringES2015` /
  `arrayToLocaleStringES2020` and any case where a fresh literal targets an
  indexed-access or keyof on a Lazy lib-namespace interface.

## Intent

`contextual_type_allows_literal_inner` widens fresh string literals when the
contextual type is `keyof Lazy(NumberFormatOptionsStyleRegistry)` or
`IndexAccess(Lazy(NumberFormatOptions), 'style')`. The first-pass
`evaluate_type_with_env` returns the same type because the lib `Lazy` def
hasn't been registered in `TypeEnvironment` yet, and subsequent classification
returns `NotAllowed`, causing `'currency'` to widen to `string`.

This fix:

1. **`keyof Lazy` fallback**: when the keyof inner is a Lazy with no progress
   from `evaluate_type_with_env`, force `ensure_relation_input_ready` on the
   inner, re-evaluate, and retry the keyof evaluation.
2. **`IndexAccess(Lazy, "key")` fallback**: when the indexed-access object is
   a Lazy and the index is a literal string, resolve via `ensure_relation_input_ready`
   then look up the property type through the existing solver
   `contextual_property_type` query and recurse.

Both fallbacks respect the architecture: no checker pattern-matching of solver
internals; resolution goes through `evaluate_type_with_env` and the
`contextual_property_type` boundary helper.

## Files Touched

- `crates/tsz-checker/src/state/type_analysis/computed_helpers.rs` (~40 LOC)
- `crates/tsz-checker/tests/contextual_literal_keyof_lib_tests.rs` (new, ~95 LOC)
- `crates/tsz-checker/Cargo.toml` (+4 LOC test entry)

## Verification

- `cargo nextest run -p tsz-checker --lib` — 2836 passed
- `cargo nextest run -p tsz-checker --test contextual_literal_keyof_lib_tests` — 4 passed (new)
- `./scripts/conformance/conformance.sh run --max 200` — 200/200 (no regressions in smoke)
- `arrayToLocaleStringES5` test now passes (was failing); ES2015/ES2020 still fail
  on the call-argument-object-literal path (separate code path; needs follow-up).

## Repros that now pass

```ts
// Variable annotation with alias to lib indexed-access
type S = Intl.NumberFormatOptions["style"];
const x: S = "currency"; // OK (was TS2322)

// Direct indexed-access on lib namespace
const y: Intl.NumberFormatOptions["style"] = "currency"; // OK (was TS2322)

// Intersection of lib namespace with `{}`
const z: Intl.NumberFormatOptions & {} = { style: "currency" }; // OK (was TS2322)
```
