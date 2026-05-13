# fix(audit): follow up missed-review threads (#5701, #5845, #5867, #5694, #5677)

- **Date**: 2026-05-12
- **Branch**: `codex/audit-followup-dts-20260512`
- **PR**: https://github.com/mohsen1/tsz/pull/6073
- **Status**: ready
- **Workstream**: review audit follow-ups

## Intent

Close five declaration-emitter missed-review clusters from the last-500-PR audit:

- `#5701` (JSDoc `@typedef ... default` alias collision with existing type names)
- `#5845` (`export =` JS files misclassified as native ESM)
- `#5867` (JS late-bound function namespace alias collisions and scope leakage)
- `#5694` (JSDoc generic/reference parser edge cases in quotes and same-line tags)
- `#5677` (rest tuple expansion can collide with pre-existing parameter names and skip unlabeled tuple elements)

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

- review comments left on #5867:
  - stop reserving JS late-bound namespace local names in global
    `reserved_names`; those names are namespace-scoped and should not force
    unrelated top-level alias renames later in the file.
  - harden collision alias generation for namespace members so synthetic names
    are unique against both global reserved names and already-known namespace
    member names.

- regression coverage:
  - updated the existing JS late-bound reserved-name test to assert that a
    member name from one namespace no longer forces a synthetic rename in a
    different namespace.
  - added a new regression proving collision fallback skips existing namespace
    member names (e.g. chooses `normal_2` when `normal_1` already exists).

- review comments left on #5694:
  - update generic-angle-bracket balancing to ignore `<`/`>` characters inside
    quoted string literal segments (with escape handling), so valid references
    like `Test<\"a>b\">` are preserved as name-like JSDoc types.
  - update same-line JSDoc tag trimming to detect any whitespace (not just a
    literal space) before `@`, so `@template ...\t@typedef ...` forms are
    parsed cleanly.

- regression coverage:
  - added a regression for quoted `>` in generic string literal arguments.
  - added a regression for tab-separated same-line `@template` + `@typedef`.

- review comments left on #5677:
  - thread rest-tuple expansion parameter-name de-dup through the outer returned
    function parameter list so expanded tuple labels are unique against
    parameters that already exist in the same signature.
  - apply the same collision-safe naming in function-type-text rest tuple
    expansion by seeding existing parameter names before expansion.
  - support unlabeled tuple elements during rest expansion by synthesizing
    stable parameter names (`arg0`, `arg1`, ...) instead of bailing out to the
    unexpanded rest parameter.

- regression coverage:
  - added a regression proving `return function (a: number, ...args: [a: string, ...])`
    expands as `a, a_1, ...` rather than emitting duplicate `a` declarations.
  - added a regression proving unlabeled tuple aliases (e.g. `[string, number]`)
    expand to synthesized positional parameters.

## Files Touched

- `crates/tsz-emitter/src/declaration_emitter/helpers/emit_node.rs`
- `crates/tsz-emitter/src/declaration_emitter/helpers/jsdoc.rs`
- `crates/tsz-emitter/src/declaration_emitter/helpers/late_bound_function_analysis.rs`
- `crates/tsz-emitter/src/declaration_emitter/helpers/returned_function_initializer.rs`
- `crates/tsz-emitter/src/declaration_emitter/tests/simple_declarations.rs`
- `crates/tsz-emitter/src/declaration_emitter/tests/type_formatting.rs`
- `docs/plan/claims/codex-review-audit-dts-followup-20260512.md`

## Verification

- `cargo test -p tsz-emitter test_js_default_typedef_after_default_identifier_export_uses_export_name -- --nocapture`
  - result: `1 passed; 0 failed`
- `cargo test -p tsz-emitter test_js_export_equals_keeps_commonjs_function_expando_namespace_members -- --nocapture`
  - result: `1 passed; 0 failed`
- `cargo test -p tsz-emitter test_js_late_bound_function_reserved_alias_uses_keyword_name -- --nocapture`
  - result: `1 passed; 0 failed`
- `cargo test -p tsz-emitter test_js_late_bound_function_alias_generation_avoids_existing_namespace_members -- --nocapture`
  - result: `1 passed; 0 failed`
- `cargo test -p tsz-emitter test_js_variable_preserves_generic_jsdoc_type_reference_with_gt_in_string_literal -- --nocapture`
  - result: `1 passed; 0 failed`
- `cargo test -p tsz-emitter test_js_template_same_line_typedef_with_tab_separator_trims_following_tag -- --nocapture`
  - result: `1 passed; 0 failed`
- `cargo test -p tsz-emitter test_direct_returned_function_expression_expands_rest_tuple_aliases -- --nocapture`
  - result: `1 passed; 0 failed`
- `cargo test -p tsz-emitter test_direct_returned_function_expression_rest_tuple_alias_avoids_existing_param_name_collision -- --nocapture`
  - result: `1 passed; 0 failed`
- `cargo test -p tsz-emitter test_direct_returned_function_expression_expands_unlabeled_rest_tuple_alias_elements -- --nocapture`
  - result: `1 passed; 0 failed`
- `cargo fmt --all`
  - result: success
