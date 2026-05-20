//! Function-declaration / function-expression parameter prologue should
//! preserve binding patterns natively at ES2015+.
//!
//! Structural rule: when a function parameter's default initializer (or any
//! element-default inside its binding pattern) requires downleveling (e.g.
//! contains `??` / `?.` and the target is below ES2020), the body prologue
//! must emit `var <pattern> = <renamed_param> === void 0 ? <init> : <renamed_param>;`
//! at ES2015+ targets. ES5 keeps the legacy property-access lowering.
//!
//! The arrow-function path already followed this rule (covered by
//! `arrow_parameter_prologue_tests`); these tests cover the
//! `function f(...) {}` and `const f = function(...) {}` shapes.

use tsz_common::common::ScriptTarget;
use tsz_emitter::output::printer::{PrintOptions, lower_and_print};
use tsz_parser::parser::ParserState;

fn emit_with_target(source: &str, target: ScriptTarget) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    lower_and_print(
        &parser.arena,
        root,
        PrintOptions {
            target,
            ..PrintOptions::default()
        },
    )
    .code
}

// -------------------------------------------------------------------------
// Function declaration: top-level default with nullish coalescing
// -------------------------------------------------------------------------

#[test]
fn es2015_function_decl_nullish_param_default_keeps_object_destructuring_native() {
    let source = "declare function getThing(): { a: number } | undefined;\n\
                  declare const fallback: { a: number };\n\
                  function f({ a } = getThing() ?? fallback) {\n  return a;\n}\n";
    let output = emit_with_target(source, ScriptTarget::ES2015);

    assert!(
        output.contains("var { a } = _a === void 0 ?"),
        "Function declaration at ES2015 should preserve `var {{ a }} = ...` natively.\nOutput:\n{output}"
    );
    assert!(
        !output.contains(", a = "),
        "Function declaration at ES2015 must not walk the pattern out into property-access assignments.\nOutput:\n{output}"
    );
}

#[test]
fn es2017_function_decl_nullish_param_default_keeps_object_destructuring_native() {
    // Renamed pattern/keys to prove the fix is structural, not name-keyed.
    let source = "declare function fetchPair(): { left: string; right: string } | undefined;\n\
                  declare const defaults: { left: string; right: string };\n\
                  function build({ left, right } = fetchPair() ?? defaults) {\n  return left + right;\n}\n";
    let output = emit_with_target(source, ScriptTarget::ES2017);

    assert!(
        output.contains("var { left, right } = _a === void 0 ?"),
        "Function declaration at ES2017 should preserve native destructuring of any property names.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("left = ") || !output.contains("right = "),
        "Function declaration at ES2017 must not walk the pattern out into per-property assignments.\nOutput:\n{output}"
    );
}

#[test]
fn es2015_function_decl_optional_chain_param_default_keeps_destructuring_native() {
    let source = "declare const provider: { source(): { a: number } | undefined } | undefined;\n\
                  declare const fallback: { a: number };\n\
                  function f({ a } = provider?.source() ?? fallback) {\n  return a;\n}\n";
    let output = emit_with_target(source, ScriptTarget::ES2015);

    assert!(
        output.contains("var { a } = _a === void 0 ?"),
        "Function declaration default containing `?.` should still keep the destructuring native.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("a = _b.a"),
        "Function declaration must not walk the pattern out into property-access assignments.\nOutput:\n{output}"
    );
}

#[test]
fn es2015_function_decl_element_default_uses_nullish_keeps_destructuring_native() {
    // Pattern has no top-level default; the inner element default needs nullish lowering.
    let source = "declare function getThing(): number | undefined;\n\
                  declare const fallback: number;\n\
                  function f({ a = getThing() ?? fallback }: { a?: number }) {\n  return a;\n}\n";
    let output = emit_with_target(source, ScriptTarget::ES2015);

    assert!(
        output.contains("var { a = ") && output.contains(" } = _a;"),
        "Element-default nullish lowering must keep the outer destructuring native at ES2015.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var _b = _a.a"),
        "Element-default lowering must not first split the pattern out.\nOutput:\n{output}"
    );
}

#[test]
fn es2015_function_decl_array_destructuring_with_nullish_default_stays_native() {
    let source = "declare function getThing(): [number, number] | undefined;\n\
                  declare const fallback: [number, number];\n\
                  function f([a, b] = getThing() ?? fallback) {\n  return a + b;\n}\n";
    let output = emit_with_target(source, ScriptTarget::ES2015);

    assert!(
        output.contains("var [a, b] = _a === void 0 ?"),
        "Array binding pattern with a nullish default should stay native at ES2015.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("a = _b[0]"),
        "Array binding pattern must not be lowered to index-access assignments at ES2015.\nOutput:\n{output}"
    );
}

// -------------------------------------------------------------------------
// Function expression: same structural rule
// -------------------------------------------------------------------------

#[test]
fn es2015_function_expression_nullish_param_default_keeps_destructuring_native() {
    let source = "declare function getThing(): { a: number } | undefined;\n\
                  declare const fallback: { a: number };\n\
                  const handler = function ({ a } = getThing() ?? fallback) {\n  return a;\n};\n";
    let output = emit_with_target(source, ScriptTarget::ES2015);

    assert!(
        output.contains("= function (_a) {") && output.contains("var { a } = _a === void 0 ?"),
        "Function expression at ES2015 should preserve `var {{ a }} = ...` natively.\nOutput:\n{output}"
    );
    assert!(
        !output.contains(", a = "),
        "Function expression at ES2015 must not walk the pattern out into property-access assignments.\nOutput:\n{output}"
    );
}

#[test]
fn es2017_function_expression_with_optional_chain_default_keeps_destructuring_native() {
    // Different property names + ES2017 target to prove the rule is structural.
    let source = "declare const source: { read(): { id: string } | undefined } | undefined;\n\
                  declare const defaults: { id: string };\n\
                  const read = function ({ id } = source?.read() ?? defaults) {\n  return id;\n};\n";
    let output = emit_with_target(source, ScriptTarget::ES2017);

    assert!(
        output.contains("var { id } = _a === void 0 ?"),
        "Function expression at ES2017 should preserve native destructuring with renamed keys.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("id = _b.id"),
        "Function expression must not walk the pattern out into property-access assignments.\nOutput:\n{output}"
    );
}

// -------------------------------------------------------------------------
// ES5 regression checks: legacy lowering must remain intact
// -------------------------------------------------------------------------

#[test]
fn es5_function_decl_nullish_param_default_still_uses_property_access_lowering() {
    let source = "declare function getThing(): { a: number } | undefined;\n\
                  declare const fallback: { a: number };\n\
                  function f({ a } = getThing() ?? fallback) {\n  return a;\n}\n";
    let output = emit_with_target(source, ScriptTarget::ES5);

    // ES5 keeps the ES5-style lowering: two temps plus a property read.
    assert!(
        output.contains("var _b = _a === void 0"),
        "ES5 must still allocate a pattern temp for the body prologue.\nOutput:\n{output}"
    );
    assert!(
        output.contains("a = _b.a"),
        "ES5 must still walk the pattern out into property-access assignments.\nOutput:\n{output}"
    );
}

#[test]
fn es5_function_expression_nullish_param_default_still_uses_property_access_lowering() {
    let source = "declare function getThing(): { a: number } | undefined;\n\
                  declare const fallback: { a: number };\n\
                  const handler = function ({ a } = getThing() ?? fallback) {\n  return a;\n};\n";
    let output = emit_with_target(source, ScriptTarget::ES5);

    assert!(
        output.contains("var _b = _a === void 0"),
        "ES5 must still allocate a pattern temp for function-expression body prologues.\nOutput:\n{output}"
    );
    assert!(
        output.contains("a = _b.a"),
        "ES5 must still walk the pattern out into property-access assignments.\nOutput:\n{output}"
    );
}

// -------------------------------------------------------------------------
// Object rest path is unchanged: tsz keeps the legacy lowering at non-ES5
// because object rest needs the `__rest` helper at ES2017 and below. This
// is out of scope for the present fix but the test pins existing behavior
// so the new branch does not silently take over the rest case.
// -------------------------------------------------------------------------

#[test]
fn es2015_function_decl_object_rest_default_preserves_existing_lowering() {
    let source = "declare function getThing(): { a: number; b: number } | undefined;\n\
                  declare const fallback: { a: number; b: number };\n\
                  function f({ a, ...rest } = getThing() ?? fallback) {\n  return a;\n}\n";
    let output = emit_with_target(source, ScriptTarget::ES2015);

    // Object rest at ES2015 still uses the legacy `if (...) _a = ...` form
    // because `__rest` is the helper used for the rest collection.
    assert!(
        output.contains("var a = _a.a, rest = __rest(_a"),
        "Object rest at ES2015 must continue to use the __rest lowering path.\nOutput:\n{output}"
    );
}

// -------------------------------------------------------------------------
// No-prologue baseline: when no downlevel is required, parameter syntax is
// emitted entirely native — no pattern temp and no body prologue at all.
// -------------------------------------------------------------------------

#[test]
fn es2020_function_decl_nullish_param_default_is_emitted_native() {
    let source = "declare function getThing(): { a: number } | undefined;\n\
                  declare const fallback: { a: number };\n\
                  function f({ a } = getThing() ?? fallback) {\n  return a;\n}\n";
    let output = emit_with_target(source, ScriptTarget::ES2020);

    assert!(
        output.contains("function f({ a } = getThing() ?? fallback) {"),
        "ES2020 keeps the source parameter syntax untouched.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var _a"),
        "ES2020 must not synthesize a parameter prologue when no downlevel is needed.\nOutput:\n{output}"
    );
}

// -------------------------------------------------------------------------
// Additional structural shapes: nested pattern, property rename, ES2021
// -------------------------------------------------------------------------

#[test]
fn es2015_function_decl_nested_object_binding_pattern_stays_native() {
    let source = "declare function getThing(): { a: { b: number } } | undefined;\n\
                  declare const fallback: { a: { b: number } };\n\
                  function f({ a: { b } } = getThing() ?? fallback) {\n  return b;\n}\n";
    let output = emit_with_target(source, ScriptTarget::ES2015);

    assert!(
        output.contains("var { a: { b } } = _a === void 0 ?"),
        "Nested binding pattern must stay native in the body prologue.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("b = _b.a.b") && !output.contains("a = _b.a"),
        "Nested binding pattern must not be walked out into property-access assignments.\nOutput:\n{output}"
    );
}

#[test]
fn es2015_function_decl_property_rename_with_element_default_stays_native() {
    // Renamed key (`a: x`) plus element-default whose initializer needs lowering.
    let source = "declare function getThing(): number | undefined;\n\
                  declare const fallback: number;\n\
                  function f({ a: x = getThing() ?? fallback }: { a?: number }) {\n  return x;\n}\n";
    let output = emit_with_target(source, ScriptTarget::ES2015);

    assert!(
        output.contains("var { a: x = ") && output.contains(" } = _a;"),
        "Property-rename plus element-default must preserve the rename in the native prologue.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var _b = _a.a"),
        "Property-rename pattern must not split into intermediate property reads.\nOutput:\n{output}"
    );
}

#[test]
fn es2021_function_decl_nullish_param_default_still_native_above_es2020() {
    // ES2021 still triggers nullish lowering above ES2020? No — `??` is ES2020,
    // so at ES2021+ the source survives untouched. The branch under test is
    // shared across all non-ES5 targets, and this pins the rule that pattern
    // shape is preserved equally for ES2021 / ES2022.
    let source = "declare function getThing(): { a: number } | undefined;\n\
                  declare const fallback: { a: number };\n\
                  function f({ a } = getThing() ?? fallback) {\n  return a;\n}\n";
    let output = emit_with_target(source, ScriptTarget::ES2021);

    assert!(
        output.contains("function f({ a } = getThing() ?? fallback) {"),
        "ES2021 keeps the source parameter syntax untouched.\nOutput:\n{output}"
    );
}
