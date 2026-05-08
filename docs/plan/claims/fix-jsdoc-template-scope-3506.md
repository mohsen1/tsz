# fix(checker): JSDoc `@template` declarations no longer suppress unrelated unused-locals (#3506)

- **Date**: 2026-05-08
- **Branch**: `fix/jsdoc-template-scope-3506`
- **PR**: TBD
- **Status**: claim
- **Workstream**: JSDoc scoping

## Intent

Issue #3506 documents two related bugs from the same root cause —
`@template` names treated as file-wide text matches:

1. An unrelated value local named `T` is considered "used" because some
   later JSDoc comment contains `@template T`. tsc emits TS6133 for the
   unused local; tsz silently suppressed it.
2. A `@template T` on one function suppresses TS2304 for `@param {T}`
   on a different function.

This PR ships fix 1 and improves the JSDoc parser plumbing that fix 2
will need. Fix 2 itself is held back because the file-wide check at
`diagnostics.rs::source_file_declares_jsdoc_template` is the same
mechanism that lets a `function Multimap() { … } /** @template K …
*/` constructor's templates flow into `Multimap.prototype.get`'s
JSDoc — narrowing it without modeling that prototype-method scope
regresses
`dispatch_tests::jsdoc_constructor_template_scope_flows_to_prototype_methods`.

## Changes

- `crates/tsz-checker/src/types/type_checking/unused.rs` —
  drop the `format!("@template {name}")` pattern from the
  JSDoc-reference probe in `is_symbol_used_in_jsdoc`. `@template`
  *declares* a scoped type parameter, it does not *reference* an
  existing symbol; treating it as a use silently suppressed TS6133
  for unrelated locals/imports/value declarations of the same name.
  As a side-benefit, this also closes the substring bug where
  `@template TypeParam` matched as a hit for a local named `T`.

- `crates/tsz-checker/src/jsdoc/diagnostics.rs` — extend the JSDoc
  line-trim helpers in `jsdoc_param_return_type_spans` and the
  `is_param_tag` detector to strip `/**` / `*/` framing in addition
  to `*`. Without it, single-line JSDoc tags like
  `/** @param {T} y */` were silently skipped by every
  `@param`/`@returns` validator. Behaviour-neutral on its own (the
  file-wide template check still suppresses TS2304 for those
  references) but lays groundwork for the proper scope fix.

- `crates/tsz-checker/tests/unused_jsdoc_template_declaration_tests.rs`
  — two new tests locking the `unused.rs` fix:
  - `jsdoc_template_does_not_mark_unrelated_value_local_as_used` (the
    issue's repro 1).
  - `jsdoc_template_with_longer_param_name_does_not_suppress_other_locals`
    (substring-prefix anti-regression: `@template TypeParam` must
    not mark `T` as used).

- `crates/tsz-checker/Cargo.toml` — register the new integration
  test target.

## Out of scope

Repro 2 (cross-function TS2304) needs a scoping model that
distinguishes free functions (no flow) from constructor + prototype
methods (flow through). The naive "current comment only" check
regresses the constructor-prototype test in
`dispatch_tests::jsdoc_constructor_template_scope_flows_to_prototype_methods`.
A follow-up PR should add a JSDoc-class scope walker that mirrors
`source_file_declares_jsdoc_template_at`'s class span check for
prototype-method ownership of free-function constructors.

## Verification

- `cargo nextest run -p tsz-checker` — 7202 / 7202 tests pass.
- New tests: `cargo nextest run -p tsz-checker --test unused_jsdoc_template_declaration_tests`
  — 2/2 pass.
- Manual repro 1: `tsz` now emits TS6133 for both `T` and `f`,
  matching tsc.
