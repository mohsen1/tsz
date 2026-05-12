# fix(audit): follow up missed-review threads (#5701, #5845)

- **Date**: 2026-05-12
- **Branch**: `codex/audit-followup-dts-20260512`
- **PR**: https://github.com/mohsen1/tsz/pull/6073
- **Status**: ready
- **Workstream**: review audit follow-ups

## Intent

Close two declaration-emitter missed-review clusters from the last-500-PR audit:

- `#5701` (JSDoc `@typedef ... default` alias collision with existing type names)
- `#5845` (`export =` JS files misclassified as native ESM)

## Changes

- review comments left on #5701:
  - changed JSDoc default-typedef emission to synthesize a non-conflicting local
    type alias name (`<defaultExportName>_default`, with numeric fallback when
    needed) instead of reusing the default-export value name.
  - emit the default typedef as:
    - `type <alias> = ...;`
    - `export { type <alias> as default };`
    to preserve the default-type surface without creating duplicate identifier
    conflicts against class/interface/type declarations.
  - reserved the synthesized alias name in emitter name tracking to avoid
    secondary collisions in the same file.

- review comments left on #5845:
  - tightened `source_file_has_native_esm_syntax` so `EXPORT_ASSIGNMENT` only
    counts as native ESM when the assignment is not `export =`.
  - this keeps JS `export =` files on CommonJS analysis paths used by late-bound
    expando and export synthesis helpers.

- regression coverage:
  - updated the existing default-typedef hoist test to assert collision-free
    alias + type-only default export emission.
  - added a new regression proving `function foo; foo.label = ...; export = foo;`
    still emits merged namespace expando members under `export =`.

## Files Touched

- `crates/tsz-emitter/src/declaration_emitter/helpers/emit_node.rs`
- `crates/tsz-emitter/src/declaration_emitter/helpers/jsdoc.rs`
- `crates/tsz-emitter/src/declaration_emitter/tests/simple_declarations.rs`
- `docs/plan/claims/codex-review-audit-dts-followup-20260512.md`

## Verification

- `cargo test -p tsz-emitter test_js_default_typedef_after_default_identifier_export_uses_export_name -- --nocapture`
  - result: `1 passed; 0 failed`
- `cargo test -p tsz-emitter test_js_export_equals_keeps_commonjs_function_expando_namespace_members -- --nocapture`
  - result: `1 passed; 0 failed`
- `cargo fmt --all`
  - result: success
