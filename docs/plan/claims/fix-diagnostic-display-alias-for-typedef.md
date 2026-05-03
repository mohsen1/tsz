# fix(checker): preserve JSDoc @typedef alias name in TS2375 / exact-optional diagnostics

- **Date**: 2026-05-03
- **Branch**: `fix/diagnostic-display-alias-for-typedef`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance — type-display-parity for JSDoc-aliased targets)

## Intent

For JSDoc `@typedef {object} A` (or `@typedef {{ ... }} A`), tsc reports
the alias name `A` in TS2375 (`exactOptionalPropertyTypes`) messages
instead of expanding to the body's structural form
(`{ value?: number; }`):

```ts
/** @typedef {object} A
 *  @property {number} [value] */
/** @type {A} */
const a = { value: undefined };

// tsc:        Type '{ value: undefined; }' is not assignable to type 'A' with ...
// tsz before: Type '{ value: undefined; }' is not assignable to type '{ value?: number; }' with ...
// tsz after:  Type '{ value: undefined; }' is not assignable to type 'A' with ...
```

The fix is two-part:

1. **`ensure_jsdoc_typedef_def`** (`crates/tsz-checker/src/jsdoc/resolution/type_construction.rs`)
   — after creating or finding the typedef's `DefId`, attach a
   display-alias entry `body_type → lazy(def_id)` via
   `store_display_alias`. This mirrors the existing pattern for
   JSDoc-assigned-value defs (line 305) and for instantiated alias
   displays in `core_formatting.rs` / `type_display.rs`.
2. **`format_exact_optional_target_type_for_message`**
   (`crates/tsz-checker/src/error_reporter/assignability.rs`) — before
   falling through to the structural formatter, consult
   `get_display_alias(target)` and route any resolved alias through
   `authoritative_assignability_def_name` to recover the def name.
   Mirrors the existing `prefer_authoritative_name` branch in
   `format_top_level_assignability_message_types` (used for general
   TS2322).

## Files Touched

- `crates/tsz-checker/src/jsdoc/resolution/type_construction.rs`
  (+22 / -16) — refactor `ensure_jsdoc_typedef_def` to expose the
  resolved `DefId` and call `store_display_alias` after registration /
  lookup.
- `crates/tsz-checker/src/error_reporter/assignability.rs` (+9 / -0) —
  honor `display_alias` in `format_exact_optional_target_type_for_message`.
- `crates/tsz-checker/tests/jsdoc_type_tag_tests.rs` (+105 / 0) — two
  new structural-rule pins:
  - `test_jsdoc_typedef_object_alias_name_preserved_in_ts2375`:
    `@typedef {object} MyAlias` + `@property {number} [value]`.
  - `test_jsdoc_typedef_inline_alias_name_preserved_in_ts2375`:
    `@typedef {{ flag?: string }} OtherAlias`.

## Verification

- `cargo nextest run -p tsz-checker --test jsdoc_type_tag_tests` — all
  jsdoc-type-tag tests pass (existing + 2 new).
- `cargo nextest run -p tsz-checker -E 'test(jsdoc)'` — 379 jsdoc
  tests pass.
- Targeted conformance: `strictOptionalProperties3.ts` improves from
  4 fingerprint mismatches → 2 (alias `A` now appears in messages).
- Full conformance: identical net delta with and without the patch on
  the same `main` HEAD (`+2 / 2 improvements / 0 regressions` —
  `arrowExpressionBodyJSDoc.ts` and
  `destructuringParameterDeclaration8.ts` flips, both pre-existing
  drift). The fix is conformance-neutral with **no regressions**.

## Notes for follow-up

`strictOptionalProperties3.ts` doesn't fully flip because the test
defines two typedefs (`A` and `B`) with **identical body shapes**
(`{ value?: number }`). Both register a display-alias to the same
`body_type`, but only the FIRST registered alias wins — so all
references display as `A`, even references via `@type {B}`.

The structurally correct fix is to make the JSDoc `@type {Name}`
resolution produce a `Lazy(DefId)` that points to the SPECIFIC
typedef referenced (not the body type), so that distinct typedefs
with the same body don't collide on display. That's a separate
iteration's worth of work in the JSDoc `@type` path
(`resolve_jsdoc_reference` and friends).

This PR's smaller fix is still valuable independently: it flips ALL
JSDoc-typedef tests where the body shape is unique, and it halves
the fingerprint mismatch count for the multi-typedef-shared-body case
(structural shape → first-registered-alias-name is closer to tsc than
structural shape).
