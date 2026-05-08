# fix(checker,binder): emit TS2300 for runtime-vs-JSDoc duplicate imports (#3508)

- **Date**: 2026-05-08
- **Branch**: `claude/nice-darwin-fuSS3`
- **PR**: TBD
- **Status**: claim
- **Issue**: #3508

## Intent

When a checked JS file declares a name twice — once via a runtime
`import { Foo }` and once via a JSDoc `@import { Foo }` — tsc reports
**TS2300** "Duplicate identifier 'Foo'" at each occurrence. tsz
currently misclassifies the runtime import as a type-only JS import
(**TS18042**) and emits no duplicate-identifier diagnostics.

Root cause: `bind_jsdoc_import_tags` re-declares the name via
`declare_symbol(... ALIAS ...)` and sets `is_type_only = true` on the
fresh alias. That fresh alias becomes the visible binding for the
file, so when the import validator queries `local_import_binding_is_type_only("Foo")`
it sees a type-only alias and falsely emits TS18042 on the runtime
import.

## Fix

1. **Binder** (`crates/tsz-binder/src/state/core.rs`): in
   `bind_jsdoc_import_tags`, skip the JSDoc declaration when the local
   name already has an alias in the current scope. The runtime alias
   stays the value-bearing binding.
2. **Checker** (`crates/tsz-checker/src/jsdoc/diagnostics_imports.rs`):
   extend the JSDoc duplicate-import pass to also collect runtime ES
   import positions. Emit TS2300 at every occurrence when at least one
   position is a JSDoc `@import` (so pure runtime/runtime collisions
   keep going through the symbol-based duplicate-identifier pass and
   are not double-reported).

## Files Touched

- `crates/tsz-binder/src/state/core.rs` (~16 LOC)
- `crates/tsz-checker/src/jsdoc/diagnostics_imports.rs` (~100 LOC,
  refactor of existing duplicate-import pass)
- `crates/tsz-checker/tests/jsdoc_import_runtime_duplicate_tests.rs`
  (new regression suite)
- `crates/tsz-checker/Cargo.toml` (one new `[[test]]` entry)

## Verification

- New unit-test crate covers: runtime named + JSDoc, runtime default +
  JSDoc, renamed runtime + JSDoc, JSDoc-only sanity, two-JSDoc
  regression guard, distinct-names sanity.
- `cargo nextest run -p tsz-checker --lib` (no regressions)
- `cargo nextest run -p tsz-binder --lib` (no regressions)
