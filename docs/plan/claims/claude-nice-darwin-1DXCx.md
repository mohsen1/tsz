# fix(binder): drop lib type-alias decls from VALUE-only shadow symbols

- **Date**: 2026-05-08
- **Branch**: `claude/nice-darwin-1DXCx`
- **PR**: TBD
- **Status**: claim
- **Workstream**: bug fixes / lib-shadow regressions

## Intent

Fix issue #4687, the regression introduced by PR #4634
("fix(binder): preserve lib's other-namespace meaning when shadowing").

When a VALUE-only module-local declaration (e.g. `const Array = 1`,
`declare const Readonly: unique symbol`) shadows a lib symbol that
contributes a TYPE meaning (e.g. lib's `interface Array<T>`,
`type Readonly<T>`), `collect_preserved_lib_meaning` was attaching the
lib's INTERFACE / TYPE_ALIAS declaration node onto the shadow symbol's
`declarations` and `declaration_arenas`. Downstream type-traversal then
walked the lib's mapped-type body as if it belonged to the user's
symbol, conflating independent type evaluations like
`Static<typeof Input>` and `Static<typeof Output>` in TypeBox-style
fixtures.

The fix preserves only the TYPE *flag* on the type side, not the
declaration node. The TYPE flag alone keeps `Array<...>` /
`Readonly<...>` resolving as types via lib-context fallbacks, while
the symbol's own `declarations` table stays free of lib-imported
type-alias bodies. The VALUE-side preservation (lib `var X` shadowed
by `interface X`) is unchanged because `var` declarations do not
drive the computed-key / mapped-type traversal that motivated this
fix, and because type-side fallback by name does not have a value-side
equivalent.

## Files Touched

- `crates/tsz-binder/src/nodes/binding.rs` (~12 LOC change in
  `collect_preserved_lib_meaning` plus expanded doc comment)
- `crates/tsz-binder/tests/lib_shadow_preservation_tests.rs` (new test
  file with 4 binder-level regression tests)
- `crates/tsz-binder/Cargo.toml` (register the new test target)
- `crates/tsz-checker/tests/lib_global_namespace_shadowing_tests.rs`
  (new typebox-style fingerprint regression test)

## Verification

- `cargo test -p tsz-binder --test lib_shadow_preservation_tests`
  (4 tests pass; same tests fail when the binder fix is reverted)
- `cargo test -p tsz-binder --tests` (470 tests pass)
- `cargo test -p tsz-checker --test lib_global_namespace_shadowing_tests`
  (6 tests pass)
- `cargo test -p tsz-checker --lib` (3764 pass; the 2 pre-existing
  failures `js_constructor_property_tests::checked_js_prototype_…` and
  `ts2300_tests::duplicate_identifier_with_default_lib_symbol_…` also
  fail on `main` and are unrelated to this fix)
- `cargo test -p tsz-solver --lib` (5713 pass)
- `cargo test -p tsz-lowering --lib` (155 pass)
