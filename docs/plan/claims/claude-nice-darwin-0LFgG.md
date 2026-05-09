# fix(checker,binder): emit TS2300 for runtime-vs-JSDoc duplicate imports

- **Date**: 2026-05-09
- **Branch**: `claude/nice-darwin-0LFgG`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance — JS/JSDoc import diagnostic parity (#3508)

## Intent

Issue #3508: when a checked JavaScript file declares the same name via both
a runtime ES `import { Foo }` and a JSDoc `@import { Foo }`, tsc emits
TS2300 "Duplicate identifier 'Foo'" at *each* occurrence. tsz instead emits
TS18042 ("type-only import in JS file") at the runtime import position and
misses the duplicate-identifier diagnostics entirely.

The structural rule, in one sentence:

> When a JSDoc `@import { X }` and a runtime ES `import { X }` (named,
> default, or namespace) introduce the same local name in the same JS
> file, tsc emits TS2300 at every occurrence; tsz must do the same and
> must not let the JSDoc tag corrupt the runtime alias's
> `is_type_only` bit.

## Files Touched

- `crates/tsz-binder/src/state/core.rs` — in `bind_jsdoc_import_tags`,
  skip the JSDoc declaration when the local name is already an alias in
  the current scope. The runtime alias keeps ownership of the binding
  and its `is_type_only` is no longer overwritten by the JSDoc tag.
- `crates/tsz-checker/src/jsdoc/diagnostics_imports.rs` — extend
  `check_jsdoc_duplicate_imports` to also walk the arena's
  `IMPORT_DECLARATION` nodes and collect runtime named/default/namespace
  local-binding positions. Emit TS2300 at every position whenever the
  same name has more than one occurrence and at least one occurrence is
  a JSDoc `@import`.
- `crates/tsz-checker/tests/issue_3508_jsdoc_runtime_duplicate_tests.rs`
  — six new regression tests covering runtime-named/JSDoc,
  runtime-default/JSDoc, renamed-runtime/JSDoc, JSDoc/JSDoc (existing
  behaviour guard), lone-JSDoc (sanity), and distinct-names (no
  collision). Tests use multiple name choices to avoid hardcoding a
  single fixture spelling (CLAUDE.md §25 #3).
- `crates/tsz-checker/Cargo.toml` — register the new `[[test]]` target.

## Verification

- `cargo nextest run -p tsz-checker --test issue_3508_jsdoc_runtime_duplicate_tests` — 6/6 pass.
- `cargo nextest run -p tsz-binder --lib` — 339/339 pass.
- `cargo nextest run -p tsz-solver --lib` — 5713/5713 pass.
- `cargo nextest run -p tsz-checker --lib` — 3764 pass; 2 pre-existing
  failures (`js_constructor_property_tests::checked_js_prototype_plain_parent_method_call_reports_ts2531`
  and `ts2300_tests::duplicate_identifier_with_default_lib_symbol_reports_lib_locations`)
  also fail on `main` and are unrelated to this change.
- `cargo nextest run -p tsz-checker --test jsdoc_import_default_and_named_tests --test jsdoc_import_multiline_tests --test jsdoc_tag_boundary_tests` —
  all 13 adjacent JSDoc tests pass.

## Notes

This is an alternative to the in-flight PRs #4673 and #4686 on the same
issue; they take a similar two-side fix shape. Net behaviour matches the
issue's expected output: for the original repro, tsz now emits

```text
a.js(1,10): error TS2300: Duplicate identifier 'Foo'.
a.js(2,15): error TS2300: Duplicate identifier 'Foo'.
```

instead of `a.js(1,10): error TS18042: …`.
