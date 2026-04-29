# fix(checker): synthesize JS function `this` type from declaration node, not merged-symbol value_declaration

- **Date**: 2026-04-29
- **Branch**: `fix/checker-js-function-this-inexistent-property`
- **PR**: #1700
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Fix missing TS2339 for `this.<inexistent>` reads inside JS function
declarations whose name shadows or merges with a lib symbol — surfaced by
`compiler/inexistentPropertyInsideToStringType.ts` (only-missing → moves
to fingerprint-only after this fix; remaining gap is the type-display
name "toString" vs "{ someValue: string; }", a separate printer concern).

Root cause: `synthesize_js_constructor_instance_type` in
`crates/tsz-checker/src/types/computation/complex_js_constructor.rs`
resolves the function symbol from `expr_idx`, then takes the symbol's
`value_declaration` (or a "checked JS constructor" alternative) to extract
the function body. When `function toString()` is declared in a JS file, its
symbol merges with one of the many ambient `toString()` overloads in
`lib.dom.d.ts` — and `value_declaration` may resolve to one of those
ambient, body-less lib declarations. `func.body.is_none()` short-circuits
synthesis to `None`, no `this`-type is pushed onto `this_type_stack` in
`function_declaration_checks.rs:574`, and the dispatch fallback for `this`
inside a JS function with `this.X = Y` patterns returns `TypeId::ANY`, so
`this.yadda` accesses become `any.yadda` — no TS2339.

Fix: when `expr_kind` is `FUNCTION_DECLARATION`/`FUNCTION_EXPRESSION`, use
`expr_node` directly to extract the function (regardless of `sym_id`).
Symbol-based resolution still applies for callers that pass an `Identifier`
or `VariableDeclaration` (e.g., `new Foo()` paths). This restores the
TS2339 emission with the correct codes; an attempt to also attach the
function's symbol to the synthesized object type so it prints by name was
reverted because it caused the synthesized constructor `this` type to
become nominal, triggering a regression in
`conformance/salsa/thisTypeOfConstructorFunctions.ts` where `() => this`
(JSDoc-annotated) and `() => Cp` (eagerly resolved) stop matching.

## Files Touched

- `crates/tsz-checker/src/types/computation/complex_js_constructor.rs` (~25 LOC reorder)
- `crates/tsz-checker/tests/js_constructor_property_tests.rs` (new test
  locking the lib-merged-symbol scenario)

## Verification

- `cargo nextest run --package tsz-checker --test js_constructor_property_tests` (60/60 pass)
- `cargo nextest run --package tsz-checker` (5549/5549 pass)
- `./scripts/conformance/conformance.sh run --filter "inexistentPropertyInsideToStringType" --verbose`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run` (full conformance, net delta)
