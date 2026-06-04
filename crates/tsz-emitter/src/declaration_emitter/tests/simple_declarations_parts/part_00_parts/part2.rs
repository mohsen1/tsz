#[test]
fn test_js_jsdoc_array_empty_args_normalizes_to_any_array() {
    // `Array.<>` (legacy JSDoc empty-args generic) should normalize to `any[]`
    // in declaration emit, matching tsc. Without the fix it surfaces as
    // `Array<>` which is not valid TypeScript.
    let output = emit_js_dts(
        r#"
/**
 * @return {Array.<>}
 */
function z() { return null; }
"#,
    );

    assert!(
        output.contains("any[]"),
        "Expected `Array.<>` to normalize to `any[]`: {output}"
    );
    assert!(
        !output.contains("Array<>"),
        "Did not expect invalid `Array<>` token in emitted type: {output}"
    );
}

#[test]
fn test_js_jsdoc_array_empty_args_in_union() {
    // The original conformance test exercises `(Array.<> | null)` as the return
    // type — the parens, union, and empty-args generic all interact. Lock in
    // that the result is `(any[] | null)`, not `(Array<> | null)`.
    let output = emit_js_dts(
        r#"
/**
 * @return {(Array.<> | null)} list of devices
 */
function z() { return null; }
"#,
    );

    assert!(
        output.contains("any[] | null") || output.contains("(any[] | null)"),
        "Expected `Array.<>` inside union to normalize: {output}"
    );
    assert!(
        !output.contains("Array<>"),
        "Did not expect raw `Array<>` token: {output}"
    );
}

#[test]
fn test_js_jsdoc_promise_empty_args_normalizes_to_promise_any() {
    // `Promise.<>` (legacy empty-args form) mirrors the Array case — should
    // normalize to `Promise<any>`, matching tsc and the bare-name fallback in
    // `resolve_jsdoc_global_implicit_any_type`.
    let output = emit_js_dts(
        r#"
/**
 * @return {Promise.<>}
 */
function p() { return Promise.resolve(); }
"#,
    );

    assert!(
        output.contains("Promise<any>"),
        "Expected `Promise.<>` to normalize to `Promise<any>`: {output}"
    );
    assert!(
        !output.contains("Promise<>"),
        "Did not expect invalid `Promise<>` token: {output}"
    );
}

#[test]
fn test_js_jsdoc_promise_star_normalizes_to_promise_any() {
    let output = emit_js_dts(
        r#"
/**
 * @return {Promise.<*>}
 */
function p() { return Promise.resolve(); }
"#,
    );

    assert!(
        output.contains("Promise<any>"),
        "Expected `Promise.<*>` to normalize to `Promise<any>`: {output}"
    );
    assert!(
        !output.contains("Promise<*>"),
        "Did not expect raw `Promise<*>` token: {output}"
    );
}

#[test]
fn test_jsdoc_nested_object_binding_params_and_promise_star() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
class Y {
    /**
     * @param {Object} error
     * @param {string?} error.reason
     * @param {Object} error.suberr
     * @param {string?} error.suberr.reason
     * @param {string?} error.suberr.code
     * @returns {Promise.<*>}
     */
    async cancel({reason, suberr}) {}
}
"#,
    );

    for expected in [
        "reason: string | null;",
        "suberr: {\n            reason: string | null;\n            code: string | null;\n        };",
        "): Promise<any>;",
    ] {
        assert!(
            output.contains(expected),
            "Expected nested JSDoc parameter output `{expected}`: {output}"
        );
    }
}

#[test]
fn test_js_trailing_jsdoc_type_aliases_are_emitted() {
    let source = r#"
export {};
/** @typedef {string | number | symbol} PropName */
/**
 * Callback
 *
 * @callback NumberToStringCb
 * @param {number} a
 * @returns {string}
 */
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export type PropName = string | number | symbol;"),
        "Expected trailing JSDoc typedef alias to be emitted: {output}"
    );
    assert!(
        output.contains("export type NumberToStringCb = (a: number) => string;"),
        "Expected trailing JSDoc callback alias to be emitted: {output}"
    );
    assert!(
        !output.contains("export {};"),
        "Did not expect an extra export scope marker once JSDoc aliases are emitted: {output}"
    );
}

#[test]
fn test_js_callback_without_return_tag_defaults_to_any() {
    let source = r#"
/**
 * Callback to be invoked when test execution is complete.
 *
 * @callback DoneCB
 * @param {number} failures - Number of failures that occurred.
 */
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("type DoneCB = (failures: number) => any;"),
        "Expected JS @callback aliases without @returns to default to any: {output}"
    );
}

#[test]
fn test_js_leading_jsdoc_typedef_before_function_is_emitted() {
    let source = r#"
/** @typedef {{x: string} | number} SomeType */
/**
 * @param {number} x
 * @returns {SomeType}
 */
export function doTheThing(x) {
  return x;
}
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export type SomeType = {\n    x: string;\n} | number;"),
        "Expected leading JSDoc typedef alias before exported function: {output}"
    );
    let alias_pos = output
        .find("export type SomeType =")
        .expect("Expected typedef alias to be emitted");
    let function_pos = output
        .find("export function doTheThing(")
        .expect("Expected exported function declaration to be emitted");
    assert!(
        alias_pos < function_pos,
        "Expected typedef alias to be emitted before the function declaration: {output}"
    );
}

#[test]
fn test_js_leading_jsdoc_typedef_before_exported_function_different_name() {
    // Verify the fix works regardless of the typedef name (anti-hardcoding: second name variant).
    let output = emit_js_dts(
        r#"
/** @typedef {string | boolean} ResultKind */
/**
 * @param {ResultKind} v
 * @returns {ResultKind}
 */
export function transform(v) {
  return v;
}
"#,
    );

    assert!(
        output.contains("export type ResultKind = string | boolean;"),
        "Expected leading typedef alias for renamed type before function: {output}"
    );
    let alias_pos = output
        .find("export type ResultKind =")
        .expect("Expected typedef alias to be emitted");
    let function_pos = output
        .find("export function transform(")
        .expect("Expected exported function to be emitted");
    assert!(
        alias_pos < function_pos,
        "Expected typedef alias before function declaration (renamed type): {output}"
    );
}

#[test]
fn test_js_non_exported_hoisted_function_preserves_typedef_comments_before_pending_aliases() {
    let output = emit_js_dts(
        r#"
/** @typedef {number} N */
/**
 * @typedef {Object} D1
 * @property {1} e Just link to {@link NS.R} this time
 */
/**
 * @param {number} value {@link N}
 */
function compute(value) {
  return value;
}
/** {@link https://example.test} */
var marker = true;
"#,
    );

    let typedef_comment_pos = output
        .find("/** @typedef {number} N */")
        .expect("Expected source typedef comment to stay before the function");
    let function_pos = output
        .find("declare function compute(value: number): number;")
        .expect("Expected non-exported function declaration");
    let var_pos = output
        .find("declare var marker: boolean;")
        .expect("Expected following variable declaration");
    let alias_pos = output
        .find("type N = number;")
        .expect("Expected pending alias");

    assert!(
        typedef_comment_pos < function_pos && function_pos < var_pos && var_pos < alias_pos,
        "Non-exported JSDoc-hoisted functions should keep typedef comments before the function and defer aliases after declarations: {output}"
    );
    assert!(
        output.contains("type D1 = {") && output.contains("e: 1;"),
        "Expected object typedef alias to still be emitted from the deferred pass: {output}"
    );
}

#[test]
fn test_js_leading_jsdoc_typedef_before_exported_class_is_emitted() {
    // Leading @typedef before an exported class should also be emitted before the class.
    let output = emit_js_dts(
        r#"
/** @typedef {{id: number}} ItemShape */
export class ItemStore {
  constructor() {}
}
"#,
    );

    assert!(
        output.contains("export type ItemShape = {"),
        "Expected leading typedef alias before exported class: {output}"
    );
    let alias_pos = output
        .find("export type ItemShape =")
        .expect("Expected typedef alias to be emitted");
    let class_pos = output
        .find("export class ItemStore")
        .expect("Expected exported class to be emitted");
    assert!(
        alias_pos < class_pos,
        "Expected typedef alias before class declaration: {output}"
    );
}
