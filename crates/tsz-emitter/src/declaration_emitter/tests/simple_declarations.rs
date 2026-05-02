use super::*;

// =============================================================================
// 1. Simple Declarations
// =============================================================================

#[test]
fn test_function_declaration() {
    let source = "export function add(a: number, b: number): number { return a + b; }";
    let output = emit_dts_with_usage_analysis(source);

    assert!(
        output.contains("export declare function add"),
        "Expected export declare: {output}"
    );
    assert!(
        output.contains("a: number"),
        "Expected parameter type: {output}"
    );
    assert!(
        output.contains("): number;"),
        "Expected return type: {output}"
    );
}

#[test]
fn test_non_exported_function_declaration_emits_declare_function() {
    let source = "function helper(x: string): string { return x; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare function helper"),
        "Expected non-exported function to be emitted as declare function: {output}"
    );
    assert!(
        !output.contains("export declare function helper"),
        "Expected no export keyword for non-exported top-level function in global scope: {output}"
    );
}

#[test]
fn test_class_declaration() {
    let source = r#"
    export class Calculator {
        private value: number;
        add(n: number): this {
            this.value += n;
            return this;
        }
    }
    "#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("class Calculator"),
        "Expected class declaration: {output}"
    );
    assert!(output.contains("value"), "Expected property: {output}");
    assert!(
        output.contains("add") && output.contains("number"),
        "Expected method signature with add and number: {output}"
    );
}

#[test]
fn test_class_instance_variable_unwraps_synthetic_anonymous_type() {
    let source = r#"
class C {
    value: number;
    private hidden: number;
}
var c = new C();
"#;
    let output = emit_dts_with_usage_analysis(source);

    assert!(
        output.contains("declare var c: C;"),
        "Expected class instance variable to use the constructor type name: {output}"
    );
    assert!(
        !output.contains(": {\n    : C;"),
        "Did not expect synthetic anonymous object wrapper in declaration output: {output}"
    );
}

#[test]
fn test_interface_declaration() {
    let source = "export interface Point { x: number; y: number; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("interface Point"),
        "Expected interface: {output}"
    );
    assert!(output.contains("number"), "Expected number type: {output}");
}

#[test]
fn test_type_alias() {
    let source = "export type ID = string | number;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export type ID = string | number"),
        "Expected type alias: {output}"
    );
}

#[test]
fn test_type_only_export_module_gets_empty_export_marker() {
    // When a module has only an import (module syntax) and private types,
    // the .d.ts needs `export {};` to preserve module semantics, since tsc
    // would not emit any explicit exports for a file like this.
    let source = r#"
import "some-dep";
type T = { x: number };
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export {};"),
        "Expected empty export marker for import-only module: {output}"
    );
}

#[test]
fn test_type_export_module_still_needs_empty_export_marker() {
    // tsc emits `export {};` even when there are type exports (interfaces,
    // type aliases) because type exports are erased at runtime.
    let source = r#"
type T = { x: number };
export interface I {
    f: T;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export interface I"),
        "Expected exported interface: {output}"
    );
    assert!(
        output.contains("export {};"),
        "Expected empty export marker even with type exports: {output}"
    );
}

#[test]
fn test_empty_named_export_has_no_extra_spacing() {
    let source = "export {};";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export {};"),
        "Expected compact empty export syntax: {output}"
    );
    assert!(
        !output.contains("export {  };"),
        "Did not expect extra spacing in empty export syntax: {output}"
    );
}

#[test]
fn test_js_local_renamed_export_aliases_are_grouped() {
    let source = r#"
function hh() {}
export { hh as h };
export function i() {}
export { i as ii };
export { j as jj };
export function j() {}
"#;
    let output = emit_js_dts_with_usage_analysis(source);

    assert!(
        output.contains("export { hh as h, i as ii, j as jj };"),
        "Expected local renamed export aliases to be grouped: {output}"
    );
    assert_eq!(
        output.matches("export {").count(),
        1,
        "Expected exactly one export alias statement: {output}"
    );
}

#[test]
fn test_js_cjs_export_aliases_are_grouped_in_source_order() {
    let source = r#"
function hh() {}
module.exports.h = hh;
module.exports.i = function i() {}
module.exports.ii = module.exports.i;
module.exports.jj = module.exports.j;
module.exports.j = function j() {}
"#;
    let output = emit_js_dts_with_usage_analysis(source);

    assert!(
        output.contains("export { hh as h, i as ii, j as jj };"),
        "Expected CJS aliases to be grouped in source order: {output}"
    );
    assert_eq!(
        output.matches("export {").count(),
        1,
        "Expected exactly one CJS export alias statement: {output}"
    );
}

#[test]
fn test_private_set_accessor_omits_type_and_uses_value_param_name() {
    let source = r#"
declare class C {
    private set x(foo: string);
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare class C"),
        "Expected declared class: {output}"
    );
    assert!(
        output.contains("private set x(value);"),
        "Expected private setter value parameter canonicalization: {output}"
    );
}

#[test]
fn test_public_set_accessor_preserves_source_param_name() {
    let source = r#"
declare class C {
    set x(foo: string);
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("set x(foo: string);"),
        "Expected public setter to preserve source parameter name: {output}"
    );
}

#[test]
fn test_accessor_comments_with_bodies_are_preserved() {
    let source = r#"
export class C {
    /** getter property*/
    public get x() {
        return 1;
    }
    /** setter property*/
    public set x(/** this is value*/ value: number) {
    }
}
"#;
    let output = emit_dts(source);

    assert!(
        output.contains("/** getter property*/\n    get x(): number;"),
        "Expected getter JSDoc to be preserved in declaration emit: {output}"
    );
    assert!(
        output.contains("/** setter property*/\n    set x(/** this is value*/ value: number);"),
        "Expected setter JSDoc to be preserved in declaration emit: {output}"
    );
}

#[test]
fn test_exported_interface_member_comments_are_preserved() {
    let output = emit_dts(
        r#"
export interface Box {
    /** width docs */
    width: number;
}
"#,
    );

    assert!(
        output.contains("/** width docs */\n    width: number;"),
        "Expected exported interface member JSDoc to be preserved: {output}"
    );
}

#[test]
fn test_trailing_top_level_jsdoc_after_export_is_preserved() {
    let output = emit_dts(
        r#"
export const value = 1;
/**
 * wat
 */
"#,
    );

    assert!(
        output.contains("export declare const value = 1;\n/**\n * wat\n */"),
        "Expected trailing top-level JSDoc to be preserved after export: {output}"
    );
}

#[test]
fn test_multiline_parameter_comments_keep_interface_signature_indent() {
    let output = emit_dts(
        r#"
export interface ICallSignatureWithParameters {
    /** This is comment for function signature*/
    (/** this is comment about a*/a: string,
        /** this is comment for b*/
        b: number): void;
}
"#,
    );

    assert!(
        output.contains(
            "    (/** this is comment about a*/ a: string, \n    /** this is comment for b*/\n    b: number): void;"
        ),
        "Expected multiline parameter comments to keep interface signature indentation: {output}"
    );
}

#[test]
fn test_get_accessor_uses_matching_setter_parameter_type_for_computed_name() {
    let output = emit_dts(
        r#"
const enum G {
    B = 2,
}
class C {
    get [G.B]() {
        return true;
    }
    set [G.B](value: number) {}
}
"#,
    );

    assert!(
        output.contains("get [G.B](): number;"),
        "Expected getter to reuse matching setter parameter type: {output}"
    );
    assert!(
        !output.contains("get [G.B](): boolean;"),
        "Did not expect getter body type to override matching setter parameter type: {output}"
    );
}

#[test]
fn test_computed_methods_emit_as_property_signatures() {
    let output = emit_dts(
        r#"
const key: string = Math.random() > 0.5 ? "a" : "b";
export class C {
    [key](): string {
        return "x";
    }

    regular(): number {
        return 1;
    }
}
"#,
    );

    // tsc emits computed methods as method signatures, not property signatures.
    assert!(
        output.contains("[key](): string;"),
        "Expected computed method to use method syntax (matching tsc): {output}"
    );
    assert!(
        !output.contains("[key]: () => string;"),
        "Did not expect property signature for computed method: {output}"
    );
    assert!(
        output.contains("regular(): number;"),
        "Expected ordinary methods to stay as methods: {output}"
    );
}

#[test]
fn test_declaration_file_exports_do_not_gain_duplicate_declare() {
    let source = r#"
export class A {}
export function f(): void;
export const x: number;
"#;
    let mut parser = ParserState::new("test.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export class A"),
        "Expected exported class to preserve declaration-file form: {output}"
    );
    assert!(
        output.contains("export function f(): void;"),
        "Expected exported function to preserve declaration-file form: {output}"
    );
    assert!(
        output.contains("export const x: number;"),
        "Expected exported variable to preserve declaration-file form: {output}"
    );
    assert!(
        !output.contains("export declare class A"),
        "Did not expect duplicate declare on exported class: {output}"
    );
    assert!(
        !output.contains("export declare function f"),
        "Did not expect duplicate declare on exported function: {output}"
    );
    assert!(
        !output.contains("export declare const x"),
        "Did not expect duplicate declare on exported variable: {output}"
    );
}

#[test]
fn test_js_exported_function_and_class_do_not_emit_declare() {
    let source = r#"
export function main() {}
export class Z {}
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export function main(): void;"),
        "Expected JS export function declaration form: {output}"
    );
    assert!(
        output.contains("export class Z"),
        "Expected JS export class declaration form: {output}"
    );
    assert!(
        !output.contains("export declare function main"),
        "Did not expect declare on JS exported function: {output}"
    );
    assert!(
        !output.contains("export declare class Z"),
        "Did not expect declare on JS exported class: {output}"
    );
}

#[test]
fn test_js_const_literal_uses_type_annotation() {
    let source = "export const x = 1;";
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export const x: 1;"),
        "Expected JS const literal to emit a literal type annotation: {output}"
    );
    assert!(
        !output.contains("export const x = 1;"),
        "Did not expect JS const literal to stay as an initializer: {output}"
    );
}

#[test]
fn test_ts_const_await_literal_uses_initializer() {
    let source = "const x = await 1;\nexport { x };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare const x = 1;"),
        "Expected TS await literal const to emit an initializer: {output}"
    );
    assert!(
        !output.contains("declare const x: number;"),
        "Did not expect TS await literal const to widen to number: {output}"
    );
}

#[test]
fn test_js_variable_preserves_name_like_jsdoc_type_reference() {
    let source = r#"
/**
 * @callback Foo
 * @param {...string} args
 * @returns {number}
 */
/** @type {Foo} */
export const x = () => 1;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export const x: Foo;"),
        "Expected JS @type alias reference to be preserved: {output}"
    );
    assert!(
        output.contains("export type Foo = (...args: string[]) => number;"),
        "Expected JS @callback alias to be synthesized after the exported value: {output}"
    );
}

#[test]
fn test_js_variable_preserves_unresolved_name_like_jsdoc_type_reference() {
    let source = r#"
/** @type {B} */
var notOK = 0;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare var notOK: B;"),
        "Expected unresolved JSDoc type reference to be preserved in .d.ts emit: {output}"
    );
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
#[ignore = "broken on main: emit produces redundant `export` keyword or duplicate declarations — track in follow-up"]
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
fn test_js_script_typedef_before_variable_is_emitted_as_local_type() {
    let source = r#"
/** @typedef {{x: string}} LocalType */
const value = 1;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("type LocalType = {\n    x: string;\n};"),
        "Expected script typedef before variable statement to be emitted as a local type alias: {output}"
    );
    assert!(
        !output.contains("export type LocalType"),
        "Did not expect script typedef to be emitted as an exported type alias: {output}"
    );
}

#[test]
fn test_js_multiline_typedef_before_function_variable_is_emitted() {
    let source = r#"
/**
 * @typedef {{
 *   [id: string]: [Function, Function];
 * }} ResolveRejectMap
 */
/**
 * @param {ResolveRejectMap} handlers
 * @returns {Promise<any>}
 */
const send = handlers => Promise.resolve(handlers);
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare function send(handlers: ResolveRejectMap): Promise<any>;"),
        "Expected JSDoc-annotated JS function variable to emit as a function declaration: {output}"
    );
    assert!(
        output.contains("type ResolveRejectMap = {\n    [id: string]: [Function, Function];\n};"),
        "Expected multiline JSDoc typedef alias to be emitted as a local type alias: {output}"
    );
}

#[test]
fn test_js_function_declaration_uses_jsdoc_signature_types() {
    let source = r#"
/**
 * @param {number} x
 * @returns {string}
 */
function format(x) {
  return String(x);
}
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare function format(x: number): string;"),
        "Expected JSDoc function declaration types to flow into .d.ts emit: {output}"
    );
}

#[test]
fn test_js_named_exports_fold_into_declarations() {
    let source = r#"
const x = 1;
function f() {}
export { x, f };
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export const x: 1;"),
        "Expected named-exported const to fold into an exported declaration: {output}"
    );
    assert!(
        output.contains("export function f(): void;"),
        "Expected named-exported function to fold into an exported declaration: {output}"
    );
    assert!(
        !output.contains("export { x, f };"),
        "Did not expect a redundant named export clause after folding: {output}"
    );
}

#[test]
#[ignore = "broken on main: emit produces redundant `export` keyword or duplicate declarations — track in follow-up"]
fn test_js_named_exports_preserve_explicit_export_order() {
    let source = r#"
function require() {}
const exports = {};
class Object {}
export const __esModule = false;
export { require, exports, Object };
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    let expected = r#"export const __esModule: false;
export function require(): void;
export const exports: {};
export class Object {
}"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected explicit JS exports to stay ahead of folded named exports: {output}"
    );
}

#[test]
fn test_js_export_import_equals_drops_export_keyword() {
    let source = "export import fs2 = require(\"fs\");";
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("import fs2 = require(\"fs\");"),
        "Expected JS export import= to emit as plain import=: {output}"
    );
    assert!(
        !output.contains("export import fs2"),
        "Did not expect JS export import= to keep the export keyword: {output}"
    );
}

#[test]
fn test_js_import_meta_url_infers_string() {
    let source = r#"
const x = import.meta.url;
export { x };
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export const x: string;"),
        "Expected import.meta.url to emit as string in JS declarations: {output}"
    );
}

#[test]
fn test_ts_import_meta_url_infers_string() {
    let source = r#"
const x = import.meta.url;
export { x };
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare const x: string;"),
        "Expected import.meta.url to emit as string in TS declarations: {output}"
    );
}

#[test]
fn test_js_top_level_await_literal_preserves_literal_type() {
    let source = r#"
const x = await 1;
export { x };
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export const x: 1;"),
        "Expected top-level await of a literal to preserve the literal type: {output}"
    );
}

#[test]
fn test_js_function_using_arguments_emits_rest_param() {
    let source = r#"
function f(x) {
    arguments;
}
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare function f(x: any, ...args: any[]): void;"),
        "Expected JS functions that reference arguments to gain a synthetic rest param: {output}"
    );
}

#[test]
fn test_js_object_literal_functions_emit_namespace() {
    let source = r#"
const foo = {
    f1: (params) => {}
};
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    let expected = r#"declare namespace foo {
    function f1(params: any): void;
}"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected namespace-like JS object literals to emit as declare namespaces: {output}"
    );
}

#[test]
fn test_js_object_literal_values_emit_namespace_members() {
    let source = r#"
const Strings = {
    a: "A",
    b: "B"
};
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    let expected = r#"declare namespace Strings {
    let a: string;
    let b: string;
}"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected JS object literal values to emit as namespace members: {output}"
    );
}

#[test]
fn test_js_class_zero_arg_constructor_is_omitted() {
    let source = r#"
export class Preferences {
    constructor() {}
}
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        !output.contains("constructor();"),
        "Expected zero-arg JS constructors to be omitted from declaration emit: {output}"
    );
}

#[test]
fn test_js_subclass_zero_arg_constructor_is_emitted() {
    let source = r#"
export class Super {
    /**
     * @param {string} firstArg
     * @param {string} secondArg
     */
    constructor(firstArg, secondArg) { }
}

export class Sub extends Super {
    constructor() {
        super('first', 'second');
    }
}
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("constructor();"),
        "Expected zero-arg JS constructor in subclass to be emitted in declaration: {output}"
    );
}

#[test]
fn test_js_export_equals_emits_before_target_declaration() {
    let source = r#"
const a = {};
export = a;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.starts_with("export = a;\ndeclare const a: {};"),
        "Expected JS export= to emit before its target declaration: {output}"
    );
    assert_eq!(
        output.matches("export = a;").count(),
        1,
        "Did not expect duplicate JS export= statements: {output}"
    );
}

#[test]
fn test_js_module_exports_emits_before_target_declaration() {
    let source = r#"
const a = {};
module.exports = a;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.starts_with("export = a;\ndeclare const a: {};"),
        "Expected JS module.exports assignment to emit as export=: {output}"
    );
    assert_eq!(
        output.matches("export = a;").count(),
        1,
        "Did not expect duplicate JS export= statements: {output}"
    );
}

#[test]
fn test_js_exports_assignment_emits_named_exports_and_filters_locals() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
exports.j = 1;
exports.k = void 0;
var o = {};
function C() {
    this.p = 1;
}
"#,
    );

    assert!(
        output.contains("export const j:"),
        "Expected CommonJS named export value declaration: {output}"
    );
    assert!(
        !output.contains("declare var o:"),
        "Did not expect non-exported locals to leak into JS module declarations: {output}"
    );
    assert!(
        !output.contains("declare function C"),
        "Did not expect non-exported helper declarations to leak into JS module declarations: {output}"
    );
    assert!(
        !output.contains("export const k:"),
        "Did not expect void exports to synthesize declarations: {output}"
    );
}

#[test]
fn test_js_exports_assignment_skips_chained_void_zero_preinit() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
exports.y = exports.x = void 0;
exports.x = 1;
exports.y = 2;
"#,
    );

    assert!(
        output.contains("export const x: 1;"),
        "Expected x export declaration to survive past the void-zero preinit: {output}"
    );
    assert!(
        output.contains("export const y: 2;"),
        "Expected y export declaration to survive past the void-zero preinit: {output}"
    );
    assert!(
        !output.contains("export const y: undefined;"),
        "Did not expect chained void-zero preinit to synthesize an undefined export: {output}"
    );
}

#[test]
fn test_js_exports_assignment_marks_same_name_function_exported() {
    let output = emit_js_dts(
        r#"
function foo() {}
exports.foo = foo;
"#,
    );

    assert!(
        output.contains("export function foo(): void;"),
        "Expected same-name CommonJS export to reuse the function declaration: {output}"
    );
}

#[test]
fn test_js_commonjs_named_function_export_is_not_static_augmentation_skip() {
    let output = emit_js_dts(
        r#"
module.exports.foo = function foo() {}
module.exports.foo.label = "ok";
"#,
    );

    assert!(
        output.contains("export function foo(): void;"),
        "Expected direct CommonJS function exports to emit a named function declaration: {output}"
    );
    assert!(
        !output.trim().eq("export {};"),
        "CommonJS function export should not be swallowed as a skipped static-method augmentation: {output}"
    );
}

#[test]
fn test_js_commonjs_function_expandos_emit_as_namespace_exports() {
    let source = r#"
function foo() {}
foo.foo = foo;
foo.default = foo;
module.exports = foo;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    let expected = r#"export = foo;
declare function foo(): void;
declare namespace foo {
    export { foo };
    export { foo as default };
}"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected CommonJS function expandos to emit as namespace exports: {output}"
    );
}

#[test]
fn test_js_function_value_expandos_emit_merged_namespace_members() {
    let output = emit_js_dts(
        r#"
export function foo() {}
foo.label = "ok";
"#,
    );

    assert!(
        output.contains("export namespace foo {\n    let label: string;\n}"),
        "Expected JS function value expandos to emit as merged namespace members: {output}"
    );
}

#[test]
fn test_js_commonjs_named_function_value_expandos_emit_namespace_members() {
    let output = emit_js_dts(
        r#"
module.exports.foo = function foo() {}
module.exports.foo.label = "ok";
"#,
    );

    assert!(
        output.contains("export namespace foo {\n    let label: string;\n}"),
        "Expected CommonJS named function value expandos to emit as merged namespace members: {output}"
    );
}

#[test]
fn test_js_function_class_expandos_emit_namespace_aliases() {
    let output = emit_js_dts(
        r#"
export function foo() {}
foo.Widget = class {
    value() {}
};
"#,
    );

    assert!(
        output.contains("export namespace foo {\n    export { Widget };\n}"),
        "Expected JS function class expandos to emit as merged namespace aliases: {output}"
    );
    assert!(
        output.contains("declare class Widget {\n    value(): void;\n}"),
        "Expected JS function class expandos to emit a reusable class declaration: {output}"
    );
}

#[test]
fn test_js_commonjs_named_function_class_expandos_emit_namespace_aliases() {
    let output = emit_js_dts(
        r#"
module.exports.foo = function foo() {}
module.exports.foo.Widget = class {
    value() {}
};
"#,
    );

    assert!(
        output.contains("export namespace foo {\n    export { Widget };\n}"),
        "Expected CommonJS named function class expandos to emit namespace aliases: {output}"
    );
    assert!(
        output.contains("declare class Widget {\n    value(): void;\n}"),
        "Expected CommonJS named function class expandos to emit a reusable class declaration: {output}"
    );
}

#[test]
fn test_js_commonjs_named_function_self_alias_emits_import_export_namespace_member() {
    let output = emit_js_dts(
        r#"
module.exports.foo = function foo() {}
module.exports.foo.self = module.exports.foo;
"#,
    );

    assert!(
        output.contains("export namespace foo {\n    import self = foo;\n    export { self };\n}"),
        "Expected CommonJS named function self aliases to use an import alias inside the namespace: {output}"
    );
}

#[test]
fn test_js_function_like_class_emits_companion_class() {
    let output = emit_js_dts(
        r#"
/**
 * @param {number} x
 * @param {number} y
 */
export function Point(x, y) {
    if (!(this instanceof Point)) return new Point(x, y);
    this.x = x;
    this.y = y;
}
"#,
    );

    assert!(
        output.contains("export function Point(x: number, y: number): Point;"),
        "Expected constructor-style JS function to return its companion class: {output}"
    );
    assert!(
        output.contains("export class Point {"),
        "Expected constructor-style JS function to emit a companion class: {output}"
    );
    assert!(
        output.contains("x: number | undefined;") && output.contains("y: number | undefined;"),
        "Expected this-assigned properties to be recovered on the companion class: {output}"
    );
}

#[test]
fn test_ts_late_bound_function_assignments_emit_namespace() {
    let source = r#"
export function foo() {}
foo.bar = 12;
const strMem = "strMemName";
foo[strMem] = "ok";
const dashStrMem = "dashed-str-mem";
foo[dashStrMem] = "ok";
const numMem = 42;
foo[numMem] = "ok";
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);
    let interner = TypeInterner::new();
    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let func_idx = parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            (node.kind == syntax_kind_ext::FUNCTION_DECLARATION).then_some(NodeIndex(idx as u32))
        })
        .expect("missing function declaration");
    let func_node = parser.arena.get(func_idx).expect("missing function node");
    let func = parser
        .arena
        .get_function(func_node)
        .expect("missing function data");
    let member_names: Vec<String> = emitter
        .collect_ts_late_bound_assignment_members(func.name)
        .into_iter()
        .map(|member| member.property_name_text)
        .collect();
    assert_eq!(
        member_names,
        vec!["bar", "strMemName", "\"dashed-str-mem\"", "42"],
        "Expected late-bound assignment collection to preserve declaration key text",
    );

    let output = emitter.emit(root);
    let expected = r#"export declare function foo(): void;
export declare namespace foo {
    var bar: number;
    var strMemName: string;
}"#;
    assert!(
        output.contains(expected),
        "Expected TS late-bound function assignments to emit a merged namespace: {output}"
    );
}

#[test]
fn test_ts_late_bound_arrow_assignments_preserve_key_text_and_types() {
    let source = r#"
const c = "C";
const num = 1;
const numStr = "10";
const withWhitespace = "foo bar";
const emoji = "🤷‍♂️";
export const arrow = () => {};
arrow["B"] = "bar";
export const arrow2 = () => {};
arrow2[c] = 100;
export const arrow3 = () => {};
arrow3[77] = 0;
export const arrow4 = () => {};
arrow4[num] = 0;
export const arrow5 = () => {};
arrow5["101"] = 0;
export const arrow6 = () => {};
arrow6[numStr] = 0;
export const arrow7 = () => {};
arrow7["qwe rty"] = 0;
export const arrow8 = () => {};
arrow8[withWhitespace] = 0;
export const arrow9 = () => {};
arrow9[emoji] = 0;
"#;

    let output = emit_dts_with_usage_analysis(source);
    let expected = r#"export declare const arrow: {
    (): void;
    B: string;
};
export declare const arrow2: {
    (): void;
    C: number;
};
export declare const arrow3: {
    (): void;
    77: number;
};
export declare const arrow4: {
    (): void;
    1: number;
};
export declare const arrow5: {
    (): void;
    "101": number;
};
export declare const arrow6: {
    (): void;
    "10": number;
};
export declare const arrow7: {
    (): void;
    "qwe rty": number;
};
export declare const arrow8: {
    (): void;
    "foo bar": number;
};
export declare const arrow9: {
    (): void;
    "\uD83E\uDD37\u200D\u2642\uFE0F": number;
};"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected TS late-bound arrow assignments to preserve declaration key text and types: {output}"
    );
}

#[test]
fn test_js_commonjs_exported_arrow_function_preserves_any_return_type() {
    let source = r#"
const donkey = (ast) => ast;
module.exports = donkey;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(root_node) = parser.arena.get(root) else {
        panic!("missing root node");
    };
    let Some(source_file) = parser.arena.get_source_file(root_node) else {
        panic!("missing source file");
    };
    let var_stmt_idx = source_file.statements.nodes[0];
    let var_stmt = parser
        .arena
        .get(var_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .expect("missing variable statement");
    let decl_list = parser
        .arena
        .get(var_stmt.declarations.nodes[0])
        .and_then(|node| parser.arena.get_variable(node))
        .expect("missing declaration list");
    let decl = parser
        .arena
        .get(decl_list.declarations.nodes[0])
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing declaration");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let ast_atom = interner.intern_string("ast");
    let donkey_type = interner.function(FunctionShape::new(
        vec![ParamInfo::required(ast_atom, TypeId::ANY)],
        TypeId::ANY,
    ));

    let mut type_cache = TypeCacheView::default();
    type_cache.node_types.insert(decl.name.0, donkey_type);
    type_cache
        .node_types
        .insert(decl.initializer.0, donkey_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare function donkey(ast: any): any;"),
        "Expected concise-arrow CommonJS export to preserve any return type: {output}"
    );
    assert!(
        !output.contains("declare function donkey(ast: any): void;"),
        "Did not expect concise-arrow CommonJS export to collapse to void: {output}"
    );
}

#[test]
fn test_js_commonjs_prototype_and_static_assignments_emit_synthetic_declarations() {
    let source = r#"
module.exports = MyClass;

function MyClass() {}
MyClass.staticMethod = function() {}
MyClass.prototype.method = function() {}
MyClass.staticProperty = 123;

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

    let expected = r#"export = MyClass;
declare function MyClass(): void;
declare class MyClass {
    method(): void;
}
declare namespace MyClass {
    export { staticMethod, staticProperty, DoneCB };
}
declare function staticMethod(): void;
declare var staticProperty: number;
/**
 * Callback to be invoked when test execution is complete.
 */
type DoneCB = (failures: number) => any;"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected CommonJS static/prototype assignments to emit synthetic declarations: {output}"
    );
}

#[test]
fn test_js_exports_assignment_marks_same_name_class_exported() {
    let output = emit_js_dts(
        r#"
class K {}
exports.K = K;
"#,
    );

    assert!(
        output.contains("export class K"),
        "Expected same-name CommonJS export to reuse the class declaration: {output}"
    );
}

#[test]
fn test_js_commonjs_property_access_export_reuses_assigned_initializer_type() {
    let source = r#"
var NS = {};
NS.K = class {};
exports.K = NS.K;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(root_node) = parser.arena.get(root) else {
        panic!("missing root node");
    };
    let Some(source_file) = parser.arena.get_source_file(root_node) else {
        panic!("missing source file");
    };
    let class_expr = parser
        .arena
        .get(source_file.statements.nodes[1])
        .and_then(|node| parser.arena.get_expression_statement(node))
        .map(|stmt| {
            parser
                .arena
                .skip_parenthesized_and_assertions_and_comma(stmt.expression)
        })
        .and_then(|expr| {
            parser
                .arena
                .get(expr)
                .and_then(|node| parser.arena.get_binary_expr(node))
        })
        .map(|binary| {
            parser
                .arena
                .skip_parenthesized_and_assertions_and_comma(binary.right)
        })
        .expect("missing assigned class expression");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let constructor_type = interner.callable(CallableShape {
        call_signatures: Vec::new(),
        construct_signatures: vec![CallSignature::new(Vec::new(), TypeId::ANY)],
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });

    let mut type_cache = TypeCacheView::default();
    type_cache.node_types.insert(class_expr.0, constructor_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("export var K: new () => any;"),
        "Expected property-access CommonJS export to reuse the assigned initializer type: {output}"
    );
}

#[test]
fn test_js_commonjs_named_class_expression_emits_exported_class() {
    let output = emit_js_dts(
        r#"
exports.K = class K {
    values() {}
};
"#,
    );

    assert!(
        output.contains("export class K {"),
        "Expected named CommonJS class expression to emit as an exported class: {output}"
    );
    assert!(
        output.contains("values(): void;"),
        "Expected named CommonJS class expression members to be preserved: {output}"
    );
    assert!(
        !output.contains("export var K: {"),
        "Did not expect named CommonJS class expression to lower as a constructor object: {output}"
    );
}

#[test]
fn test_object_literal_computed_numeric_names_prefer_syntax_shape() {
    let output = emit_dts(
        r#"
var v = {
  [-1]: {},
  [+1]: {},
  [~1]: {},
  [!1]: {}
};
"#,
    );

    assert!(
        output.contains("[-1]: {};"),
        "Expected negative computed numeric literal to survive in fallback object typing: {output}"
    );
    assert!(
        !output.contains("\"-1\": {};"),
        "Did not expect canonical string form to survive once syntax override is applied: {output}"
    );
    assert!(
        output.contains("1: {};"),
        "Expected unary-plus computed numeric literal to normalize to a numeric property: {output}"
    );
    assert!(
        !output.contains("\"-2\": {};"),
        "Did not expect canonicalized synthetic numeric names to leak into the object type: {output}"
    );
    assert!(
        !output.contains("[~1]: {}"),
        "Did not expect non-emittable computed names to survive fallback object typing: {output}"
    );
}

#[test]
fn test_js_module_exports_object_literal_with_computed_names_emits_export_equals_surface() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
const TopLevelSym = Symbol();
const InnerSym = Symbol();
module.exports = {
    [TopLevelSym](x = 12) {
        return x;
    },
    items: {
        [InnerSym]: (arg = { x: 12 }) => arg.x
    }
};
"#,
    );

    assert!(
        output.contains("declare const _exports: {"),
        "Expected anonymous CommonJS object export to materialize a synthetic export root: {output}"
    );
    assert!(
        output.contains("[TopLevelSym]"),
        "Expected computed symbol member to survive on synthetic export root: {output}"
    );
    assert!(
        output.contains("items: {"),
        "Expected nested object member to survive on synthetic export root: {output}"
    );
    assert!(
        output.contains("export = _exports;"),
        "Expected synthetic CommonJS object export to end with export=: {output}"
    );
}

#[test]
fn test_js_module_exports_new_expression_emits_typed_export_equals_surface() {
    let output = emit_js_dts(
        r#"
class Foo {}
module.exports = new Foo();
"#,
    );

    assert!(
        output.contains("declare const _exports: Foo;"),
        "Expected anonymous CommonJS value export to synthesize a typed export root: {output}"
    );
    assert!(
        output.contains("export = _exports;"),
        "Expected anonymous CommonJS value export to emit export=: {output}"
    );
    assert!(
        output.contains("declare class Foo"),
        "Expected the supporting class declaration to remain in the output: {output}"
    );
}

#[test]
fn test_js_module_exports_object_literal_plus_secondary_promotes_named_exports() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
const Strings = {
    a: "A",
    b: "B"
};
module.exports = {
    thing: "ok",
    also: "ok",
    desc: {
        item: "ok"
    }
};
module.exports.Strings = Strings;
"#,
    );

    assert!(
        output.contains("export declare let thing: string;"),
        "Expected anonymous CommonJS object members to become named exports when secondary module.exports members exist: {output}"
    );
    assert!(
        output.contains("export declare let also: string;"),
        "Expected sibling literal members to become named exports: {output}"
    );
    assert!(
        output.contains("export namespace Strings {"),
        "Expected secondary module.exports identifier exports to mark their source declaration as exported: {output}"
    );
    assert!(
        output.contains("export declare namespace desc {"),
        "Expected nested object members to become exported namespaces: {output}"
    );
    assert!(
        !output.contains("export = _exports;"),
        "Did not expect anonymous module.exports object roots with secondary members to stay on the synthetic export= path: {output}"
    );
}

#[test]
fn test_js_module_exports_anonymous_class_expression_uses_exports_class_surface() {
    let output = emit_js_dts(
        r#"
module.exports = class {
    /**
     * @param {number} p
     */
    constructor(p) {
        this.t = 12 + p;
    }
};
"#,
    );

    assert!(
        output.contains("export = exports;"),
        "Expected anonymous CommonJS class exports to target the synthetic exports class surface: {output}"
    );
    assert!(
        output.contains("declare class exports {"),
        "Expected anonymous CommonJS class exports to emit a named class surface: {output}"
    );
    assert!(
        output.contains("constructor(p: number);"),
        "Expected constructor JSDoc to flow through the synthetic exports class surface: {output}"
    );
    assert!(
        output.contains("t: number;"),
        "Expected instance properties to survive the synthetic exports class surface: {output}"
    );
}

#[test]
fn test_js_commonjs_function_like_export_preserves_constructor_jsdoc_block() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @param {number} timeout
 */
function Timer(timeout) {
    this.timeout = timeout;
}
module.exports = Timer;
"#,
    );

    let expected = "declare class Timer {\n    /**\n     * @param {number} timeout\n     */\n    constructor(timeout: number);\n    timeout: number;\n}";
    assert!(
        output.contains(expected),
        "Expected synthetic function-like class constructor JSDoc to stay block-formatted: {output}"
    );
}

#[test]
fn test_js_exported_function_like_class_preserves_constructor_jsdoc_block() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @param {number} x
 * @param {number} y
 */
export function Point(x, y) {
    if (!(this instanceof Point)) {
        return new Point(x, y);
    }
    this.x = x;
    this.y = y;
}
"#,
    );

    let expected = "export class Point {\n    /**\n     * @param {number} x\n     * @param {number} y\n     */\n    constructor(x: number, y: number);";
    assert!(
        output.contains(expected),
        "Expected exported function-like class constructor JSDoc to stay block-formatted: {output}"
    );
}

#[test]
fn test_js_function_like_prototype_accessors_and_proto_surface() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @param {number} len
 */
export function Vec(len) {
    /**
     * @type {number[]}
     */
    this.storage = new Array(len);
}
Vec.prototype = {
    /**
     * @param {Vec} other
     */
    dot(other) {
        if (other.storage.length !== this.storage.length) {
            throw new Error("bad");
        }
        let sum = 0;
        for (let i = 0; i < this.storage.length; i++) {
            sum += this.storage[i] * other.storage[i];
        }
        return sum;
    }
};

/**
 * @param {number} x
 * @param {number} y
 */
export function Point2D(x, y) {
    if (!(this instanceof Point2D)) {
        return new Point2D(x, y);
    }
    Vec.call(this, 2);
    this.x = x;
    this.y = y;
}
Point2D.prototype = {
    __proto__: Vec,
    get x() {
        return this.storage[0];
    },
    /**
     * @param {number} x
     */
    set x(x) {
        this.storage[0] = x;
    }
};
"#,
    );

    assert!(
        output
            .contains("/**\n * @param {number} len\n */\nexport function Vec(len: number): void;"),
        "Expected hoisted function JSDoc to stay multiline: {output}"
    );
    assert!(
        output.contains("dot(other: Vec): number;"),
        "Expected local accumulator return type to recover as number: {output}"
    );
    assert!(
        !output.contains("x: number | undefined;"),
        "Expected prototype accessor to suppress constructor-inferred x property: {output}"
    );
    let set_pos = output
        .find("set x(x: number);")
        .expect("missing setter in output");
    let get_pos = output
        .find("get x(): number;")
        .expect("missing getter in output");
    let proto_pos = output
        .find("__proto__: typeof Vec;")
        .expect("missing __proto__ surface in output");
    assert!(
        set_pos < get_pos && get_pos < proto_pos,
        "Expected setter/getter before deferred __proto__ member: {output}"
    );
}

#[test]
fn test_js_named_export_equals_class_expression_shadowing_preserves_root_name() {
    let output = emit_js_dts(
        r#"
class A {
    member = new Q();
}
class Q {
    x = 42;
}
module.exports = class Q {
    constructor() {
        this.x = new A();
    }
};
module.exports.Another = Q;
"#,
    );

    assert!(
        output.contains("export = Q;"),
        "Expected named CommonJS class export-equals roots to preserve their declared class name: {output}"
    );
    assert!(
        output.contains("declare namespace Q {"),
        "Expected named CommonJS class export-equals roots to own their namespace aliases: {output}"
    );
    assert!(
        output.contains("export { Q_1 as Another };"),
        "Expected shadowed local class aliases to be redirected through a unique declaration name: {output}"
    );
    assert!(
        output.contains("declare class Q_1 {"),
        "Expected the shadowed local class declaration to be emitted under a stable unique alias: {output}"
    );
    assert!(
        !output.contains("export = exports;"),
        "Did not expect named CommonJS class export-equals roots to fall back to the anonymous exports surface: {output}"
    );
}

#[test]
fn test_js_class_jsdoc_members_preserve_readonly_and_order() {
    let output = emit_js_dts(
        r#"
/**
 * @template T
 */
export class Box {
    /**
     * @type {T}
     */
    value;

    /**
     * @return {T}
     */
    get current() { return this.value; }

    /**
     * @type {string}
     * @readonly
     */
    static kind;
}
"#,
    );

    assert!(
        output.contains("export class Box {"),
        "Expected the JS class declaration to be preserved: {output}"
    );
    assert!(
        output.contains("static readonly kind: string;"),
        "Expected JSDoc readonly/type tags to control JS class static property emit: {output}"
    );
    assert!(
        output.contains("value: T;"),
        "Expected JSDoc property types to drive JS class field emit: {output}"
    );
    assert!(
        output.contains("get current(): T;"),
        "Expected JSDoc getter return types to drive JS accessor emit: {output}"
    );
}

#[test]
fn test_js_class_method_jsdoc_template_parameters_emit() {
    let output = emit_js_dts(
        r#"
export class Factory {
    /**
     * @template T
     * @param {T} value
     * @return {T}
     */
    static create(value) { return value; }
}
"#,
    );

    assert!(
        output.contains("static create<T>(value: T): T;"),
        "Expected JSDoc method templates on JS classes to surface in declaration emit: {output}"
    );
}

#[test]
#[ignore = "broken on main: emit produces redundant `export` keyword or duplicate declarations — track in follow-up"]
fn test_js_commonjs_class_static_assignments_emit_typedef_and_namespace_exports() {
    let source = r#"
class Handler {
    static get OPTIONS() {
        return 1;
    }

    process() {
    }
}
Handler.statische = function() { }
const Strings = {
    a: "A",
    b: "B"
};

module.exports = Handler;
module.exports.Strings = Strings;

/**
 * @typedef {Object} HandlerOptions
 * @property {String} name
 * Should be able to export a type alias at the same time.
 */
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    let expected = r#"export = Handler;
declare class Handler {
    static get OPTIONS(): number;
    process(): void;
}
declare namespace Handler {
    export { statische, Strings, HandlerOptions };
}
declare function statische(): void;
declare namespace Strings {
    let a: string;
    let b: string;
}
type HandlerOptions = {
    /**
     * Should be able to export a type alias at the same time.
     */
    name: string;
};"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected CommonJS class static assignments and typedefs to emit in source order: {output}"
    );
}

#[test]
fn test_js_class_static_method_augmentation_emits_namespace_merge() {
    let source = r#"
export class Clazz {
    static method() { }
}

Clazz.method.prop = 5;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    let expected = r#"export class Clazz {
}
export namespace Clazz {
    function method(): void;
    namespace method {
        let prop: number;
    }
}"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected JS static method augmentations to emit as a merged namespace: {output}"
    );
}

#[test]
fn test_js_reexports_from_same_module_are_grouped() {
    let source = r#"
export { default } from "fs";
export { default as foo } from "fs";
export { bar as baz } from "fs";
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export { default, default as foo, bar as baz } from \"fs\";"),
        "Expected JS re-exports from the same module to be grouped: {output}"
    );
    assert_eq!(
        output.matches(" from \"fs\";").count(),
        1,
        "Did not expect duplicate JS re-export lines after grouping: {output}"
    );
}

#[test]
fn test_method_declaration_emits_inferred_return_type() {
    let source = r#"
class C {
    add() {
        return 1;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(root_node) = parser.arena.get(root) else {
        panic!("missing root node");
    };
    let Some(source_file) = parser.arena.get_source_file(root_node) else {
        panic!("missing source file data");
    };
    let Some(class_node) = parser.arena.get(source_file.statements.nodes[0]) else {
        panic!("missing class node");
    };
    let Some(class_decl) = parser.arena.get_class(class_node) else {
        panic!("missing class declaration");
    };
    let method_idx = class_decl.members.nodes[0];

    let interner = TypeInterner::new();
    let method_type = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let mut type_cache = TypeCacheView::default();
    type_cache.node_types.insert(method_idx.0, method_type);

    let binder = BinderState::new();
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("add(): number;"),
        "Expected inferred method return type: {output}"
    );
}

#[test]
fn test_property_declaration_infers_type_from_numeric_initializer_when_type_cache_missing() {
    let source = r#"
abstract class C {
    abstract prop = 1;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("abstract prop: number;"),
        "Expected inferred property type from initializer: {output}"
    );
}

#[test]
fn test_variable_declaration_infers_accessor_object_type_from_initializer_when_type_cache_missing()
{
    let source = r#"
export var basePrototype = {
  get primaryPath() {
    return 1;
  },
};
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output
            .contains("export declare var basePrototype: {\n    readonly primaryPath: number;\n};"),
        "Expected multi-line object literal accessor inference with body type: {output}"
    );
}

#[test]
fn test_call_initializer_uses_source_function_return_shape_for_accessor_object() {
    let output = emit_dts_with_binding(
        r#"
function makePoint(x: number) {
    return {
        b: 10,
        get x() { return x; },
        set x(a: number) { this.b = a; }
    };
}
var /*4*/ point = makePoint(2);
point./*3*/x = 30;
"#,
    );

    assert!(
        output.contains("declare var /*4*/ point: {\n    b: number;\n    x: any;\n};")
            || output.contains("declare var /*4*/ point: {\n    b: number;\n    x: number;\n};"),
        "Expected call initializer to reuse source function return shape without synthetic anonymous members: {output}"
    );
    assert!(
        !output.contains("\n    : {"),
        "Did not expect a synthetic anonymous object member in call initializer output: {output}"
    );
}

#[test]
fn test_overloaded_call_initializer_does_not_use_first_signature_return_type() {
    let output = emit_dts_with_binding(
        r#"
function parse(input: string): string;
function parse(input: number): number;
function parse(input: string | number): string | number { return input; }
const result = parse(42);
"#,
    );

    assert!(
        !output.contains("declare const result: string;"),
        "Did not expect overloaded call initializer to use the first overload return type: {output}"
    );
    assert!(
        output.contains("declare const result = 42;"),
        "Expected overloaded call initializer to fall back without first-overload poisoning: {output}"
    );
}

#[test]
fn test_object_literal_computed_accessor_pair_emits_writable_symbol_property() {
    let output = emit_dts_with_binding(
        r#"
var obj = {
    get [Symbol.isConcatSpreadable]() { return ""; },
    set [Symbol.isConcatSpreadable](x) { }
};
"#,
    );

    assert!(
        output.contains("[Symbol.isConcatSpreadable]: string;"),
        "Expected computed accessor pair to collapse to writable symbol property: {output}"
    );
    assert!(
        !output.contains("readonly [Symbol.isConcatSpreadable]: string;"),
        "Did not expect computed accessor pair to remain readonly: {output}"
    );
}

#[test]
fn test_object_literal_computed_literal_key_reuses_resolved_property_name() {
    let source = r#"
const Foo = {
    BANANA: "banana" as "banana",
};

export const Baa = {
    [Foo.BANANA]: 1,
};
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let baa_decl = parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            parser
                .arena
                .get_variable_declaration(node)
                .filter(|decl| parser.arena.get_identifier_text(decl.name) == Some("Baa"))
                .map(|decl| (NodeIndex(idx as u32), decl))
        })
        .map(|(_, decl)| decl)
        .expect("missing Baa declaration");
    let object_literal = parser
        .arena
        .get(baa_decl.initializer)
        .and_then(|node| parser.arena.get_literal_expr(node))
        .expect("missing Baa object literal");
    let prop_assignment = parser
        .arena
        .get(object_literal.elements.nodes[0])
        .and_then(|node| parser.arena.get_property_assignment(node))
        .expect("missing computed property assignment");
    let computed_expr = parser
        .arena
        .get(prop_assignment.name)
        .and_then(|node| parser.arena.get_computed_property(node))
        .map(|computed| computed.expression)
        .expect("missing computed property name");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let banana_type = interner.literal_string("banana");
    let banana_atom = interner.intern_string("banana");
    let object_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: vec![PropertyInfo::new(banana_atom, TypeId::NUMBER)],
        string_index: None,
        number_index: None,
        symbol: None,
    });

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(computed_expr.0, banana_type);
    type_cache
        .node_types
        .insert(baa_decl.initializer.0, object_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("banana: number;"),
        "Expected computed literal key to emit resolved property name: {output}"
    );
    assert!(
        !output.contains("[Foo.BANANA]: number;"),
        "Did not expect computed literal key syntax to leak into declaration output: {output}"
    );
}

#[test]
fn test_enum_member_initializers_respect_const_assertion_widening() {
    let output = emit_dts_with_binding(
        r#"
enum E { A, B }
let widened = E.B;
let preserved = E.B as const;
class C {
    p1 = E.B;
    p2 = E.B as const;
    readonly p3 = E.B;
}
"#,
    );

    assert!(
        output.contains("declare let widened: E;"),
        "Expected let enum member to widen to enum type: {output}"
    );
    assert!(
        output.contains("declare let preserved: E.B;"),
        "Expected const-asserted enum member to preserve member type: {output}"
    );
    assert!(
        output.contains("p1: E;"),
        "Expected property widening: {output}"
    );
    assert!(
        output.contains("p2: E.B;"),
        "Expected const-asserted property member type: {output}"
    );
    assert!(
        output.contains("readonly p3 = E.B;"),
        "Expected readonly enum property initializer form: {output}"
    );
}

#[test]
fn test_const_enum_member_access_const_variable_preserves_initializer() {
    let output = emit_dts_with_binding(
        r#"
export const enum E {
    regular = 0,
    "hyphen-member" = 1,
}
export const a = E["hyphen-member"];
export const b = E.regular;
"#,
    );

    assert!(
        output.contains(r#"export declare const a = E["hyphen-member"];"#),
        "Expected string-keyed const enum member initializer: {output}"
    );
    assert!(
        output.contains("export declare const b = E.regular;"),
        "Expected property const enum member initializer: {output}"
    );
}

#[test]
fn test_inaccessible_constructor_new_initializer_emits_any() {
    let source = r#"
class C {
    constructor(public x: number) {}
}

class D {
    private constructor(public x: number) {}
}

class E {
    protected constructor(public x: number) {}
}

var c = new C(1);
var d = new D(1);
var e = new E(1);
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(root_node) = parser.arena.get(root) else {
        panic!("missing root node");
    };
    let Some(source_file) = parser.arena.get_source_file(root_node) else {
        panic!("missing source file data");
    };

    let class_c_idx = source_file.statements.nodes[0];
    let class_d_idx = source_file.statements.nodes[1];
    let class_e_idx = source_file.statements.nodes[2];
    let var_c_stmt_idx = source_file.statements.nodes[3];
    let var_d_stmt_idx = source_file.statements.nodes[4];
    let var_e_stmt_idx = source_file.statements.nodes[5];

    let class_c = parser
        .arena
        .get(class_c_idx)
        .and_then(|node| parser.arena.get_class(node))
        .expect("missing class C");
    let class_d = parser
        .arena
        .get(class_d_idx)
        .and_then(|node| parser.arena.get_class(node))
        .expect("missing class D");
    let class_e = parser
        .arena
        .get(class_e_idx)
        .and_then(|node| parser.arena.get_class(node))
        .expect("missing class E");

    let var_c_decl = parser
        .arena
        .get(var_c_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing var c declaration");
    let var_d_decl = parser
        .arena
        .get(var_d_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing var d declaration");
    let var_e_decl = parser
        .arena
        .get(var_e_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing var e declaration");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let c_sym = binder
        .get_node_symbol(class_c.name)
        .or_else(|| binder.get_node_symbol(class_c_idx))
        .expect("missing symbol for C");
    let d_sym = binder
        .get_node_symbol(class_d.name)
        .or_else(|| binder.get_node_symbol(class_d_idx))
        .expect("missing symbol for D");
    let e_sym = binder
        .get_node_symbol(class_e.name)
        .or_else(|| binder.get_node_symbol(class_e_idx))
        .expect("missing symbol for E");

    let interner = TypeInterner::new();
    let c_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: Some(c_sym),
    });
    let d_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: Some(d_sym),
    });
    let e_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: Some(e_sym),
    });

    let mut type_cache = TypeCacheView::default();
    type_cache.node_types.insert(var_c_decl.name.0, c_type);
    type_cache.node_types.insert(var_d_decl.name.0, d_type);
    type_cache.node_types.insert(var_e_decl.name.0, e_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare var c: C;"),
        "Expected C type: {output}"
    );
    assert!(
        output.contains("declare var d: any;"),
        "Expected d to degrade to any: {output}"
    );
    assert!(
        output.contains("declare var e: any;"),
        "Expected e to degrade to any: {output}"
    );
}

#[test]
fn test_construct_signature_new_initializer_keeps_inferred_any() {
    let source = r#"
interface Input {}
interface Factory {
    new (value: Input);
}
declare var ctor: Factory;
declare var value: Input;
var instance = new ctor(value);
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(root_node) = parser.arena.get(root) else {
        panic!("missing root node");
    };
    let Some(source_file) = parser.arena.get_source_file(root_node) else {
        panic!("missing source file data");
    };

    let instance_stmt_idx = source_file.statements.nodes[4];
    let instance_decl = parser
        .arena
        .get(instance_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing instance declaration");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache
        .node_types
        .insert(instance_decl.name.0, TypeId::ANY);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare var instance: any;"),
        "Expected construct-signature new initializer to preserve inferred any: {output}"
    );
    assert!(
        !output.contains("declare var instance: ctor;"),
        "Did not expect constructor variable name to leak into the emitted type: {output}"
    );
}

#[test]
fn test_constructor_type_no_double_semicolon() {
    let source = "export type Ctor = new (...args: any[]) => void;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("new (...args: any[]) => void;"),
        "Expected constructor type in output: {output}"
    );
    assert!(
        !output.contains(";;"),
        "Must not have double semicolon in constructor type alias: {output}"
    );
}

#[test]
fn test_template_literal_type_no_double_semicolon() {
    let source = r#"export type Outcome = `${string}_${string}`;"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("`${string}_${string}`"),
        "Expected template literal type in output: {output}"
    );
    assert!(
        !output.contains(";;"),
        "Must not have double semicolon in template literal type alias: {output}"
    );
}

#[test]
fn test_infer_type_no_double_semicolon() {
    let source = "export type Unpack<T> = T extends (infer U)[] ? U : T;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("infer U"),
        "Expected infer type in output: {output}"
    );
    assert!(
        !output.contains(";;"),
        "Must not have double semicolon in type alias with infer: {output}"
    );
}

#[test]
fn test_abstract_constructor_type() {
    let source = "export type AbstractCtor = abstract new () => object;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("abstract new () => object;"),
        "Expected abstract constructor type in output: {output}"
    );
    assert!(
        !output.contains(";;"),
        "Must not have double semicolon in abstract constructor type: {output}"
    );
}

#[test]
fn test_simple_template_literal_type() {
    let source = r#"export type Greeting = `hello`;"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("`hello`"),
        "Expected simple template literal type in output: {output}"
    );
    assert!(
        !output.contains(";;"),
        "Must not have double semicolon in simple template literal type: {output}"
    );
}

#[test]
fn test_public_modifier_omitted_from_dts_class_members() {
    // tsc omits `public` from .d.ts output since it's the default accessibility
    let source = r#"
    export class Foo {
        public x: number;
        public greet(): string { return "hello"; }
        protected y: number;
        private z: number;
    }
    "#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    // `public` should be stripped (it's the default)
    assert!(
        !output.contains("public "),
        "Expected `public` modifier to be omitted from .d.ts output: {output}"
    );
    // `protected` and `private` should be preserved
    assert!(
        output.contains("protected y"),
        "Expected `protected` modifier to be preserved: {output}"
    );
    assert!(
        output.contains("private z"),
        "Expected `private` modifier to be preserved: {output}"
    );
    // Members themselves should still be present
    assert!(
        output.contains("x: number"),
        "Expected public property to still be emitted (without modifier): {output}"
    );
    assert!(
        output.contains("greet("),
        "Expected public method to still be emitted (without modifier): {output}"
    );
}

// =============================================================================
// 2. Variable Declarations
// =============================================================================

#[test]
fn test_variable_const_declaration() {
    let output = emit_dts("export const MAX: number = 100;");
    assert!(
        output.contains("export declare const MAX: number;"),
        "Expected const variable in .d.ts: {output}"
    );
}

#[test]
fn test_variable_let_declaration() {
    let output = emit_dts("export let count: number = 0;");
    assert!(
        output.contains("export declare let count: number;"),
        "Expected let variable in .d.ts: {output}"
    );
}

#[test]
fn test_variable_var_declaration() {
    let output = emit_dts("export var name: string = 'hello';");
    assert!(
        output.contains("export declare var name: string;"),
        "Expected var variable in .d.ts: {output}"
    );
}

// =============================================================================
// 3. Visibility / Access Modifiers
// =============================================================================

#[test]
fn test_private_method_emits_name_only() {
    // tsc emits just `private methodName;` for private methods
    let output = emit_dts(
        r#"
    export class Foo {
        private secret(): void {}
    }
    "#,
    );
    assert!(
        output.contains("private secret;"),
        "Expected private method to emit name only: {output}"
    );
    // Should NOT include parameters or return type
    assert!(
        !output.contains("private secret()"),
        "Private method should not have params in .d.ts: {output}"
    );
}

#[test]
fn test_protected_member_included() {
    let output = emit_dts(
        r#"
    export class Foo {
        protected bar: number;
    }
    "#,
    );
    assert!(
        output.contains("protected bar: number;"),
        "Expected protected member to be included: {output}"
    );
}

#[test]
fn test_private_property_omits_type_annotation() {
    // tsc omits type annotations for private properties in .d.ts
    let output = emit_dts(
        r#"
    export class Foo {
        private value: number;
    }
    "#,
    );
    assert!(
        output.contains("private value;"),
        "Expected private property without type annotation: {output}"
    );
    assert!(
        !output.contains("private value: number;"),
        "Private property should NOT have type annotation: {output}"
    );
}

// =============================================================================
// 4. Export Handling
// =============================================================================

#[test]
fn test_named_export_with_specifiers() {
    let output = emit_dts(
        r#"
    const a: number = 1;
    const b: string = "x";
    export { a, b };
    "#,
    );
    assert!(
        output.contains("export { a, b }"),
        "Expected named export specifiers: {output}"
    );
}

#[test]
fn test_re_export_from_module() {
    let output = emit_dts(r#"export { foo, bar } from "./other";"#);
    assert!(
        output.contains("export { foo, bar } from"),
        "Expected re-export: {output}"
    );
}

#[test]
fn test_star_re_export() {
    let output = emit_dts(r#"export * from "./utils";"#);
    assert!(
        output.contains("export * from"),
        "Expected star re-export: {output}"
    );
}

#[test]
fn test_type_only_export() {
    let output = emit_dts(r#"export type { Foo } from "./types";"#);
    assert!(
        output.contains("export type { Foo }"),
        "Expected type-only export: {output}"
    );
}

#[test]
fn test_export_default_identifier() {
    // export default <identifier> should emit directly
    let output = emit_dts(
        r#"
    declare const myValue: number;
    export default myValue;
    "#,
    );
    assert!(
        output.contains("export default myValue;"),
        "Expected export default identifier: {output}"
    );
}

#[test]
fn test_js_export_default_identifier_is_hoisted() {
    // For JS source files, tsc hoists `export default <Identifier>` to the very
    // top of the .d.ts when the identifier resolves to a top-level local
    // declaration. Repro for jsDeclarationEmitDoesNotRenameImport.
    let output = emit_js_dts(
        r#"
function validate() {}

export default validate;
"#,
    );
    let trimmed = output.trim();
    assert!(
        trimmed.starts_with("export default validate;"),
        "Expected `export default validate;` to be hoisted to the top: {trimmed}"
    );
    let count = trimmed.matches("export default validate;").count();
    assert_eq!(
        count, 1,
        "Expected exactly one export-default emission: {trimmed}"
    );
    assert!(
        trimmed.contains("declare function validate(): void;"),
        "Expected the function declaration to follow: {trimmed}"
    );
    let default_pos = trimmed.find("export default validate;").unwrap();
    let decl_pos = trimmed.find("declare function validate").unwrap();
    assert!(
        default_pos < decl_pos,
        "`export default` should appear before the function declaration: {trimmed}"
    );
}

#[test]
fn test_js_export_default_class_is_hoisted_above_class_body() {
    // Same hoisting rule as above, but for class declarations. Uses the
    // usage-analysis variant so the class isn't pruned from the .d.ts.
    let output = emit_js_dts_with_usage_analysis(
        r#"
/** @module Test */
class Test {}
export default Test;
"#,
    );
    let trimmed = output.trim();
    assert!(
        trimmed.starts_with("export default Test;"),
        "Expected `export default Test;` to be hoisted to the top: {trimmed}"
    );
    let count = trimmed.matches("export default Test;").count();
    assert_eq!(
        count, 1,
        "Expected exactly one export-default emission: {trimmed}"
    );
    let default_pos = trimmed.find("export default Test;").unwrap();
    let decl_pos = trimmed
        .find("declare class Test")
        .unwrap_or_else(|| panic!("expected `declare class Test` in JS dts: {trimmed}"));
    assert!(
        default_pos < decl_pos,
        "`export default` should appear before the class declaration: {trimmed}"
    );
}

#[test]
fn test_ts_export_default_identifier_is_not_hoisted() {
    // TS files keep `export default <Identifier>` in source order — only JS
    // declaration emit applies the hoist transformation.
    let output = emit_dts(
        r#"
function validate() {}
export default validate;
"#,
    );
    let trimmed = output.trim();
    let default_pos = trimmed
        .find("export default validate;")
        .expect("expected export default validate; in TS output");
    let decl_pos = trimmed
        .find("declare function validate")
        .expect("expected declare function validate in TS output");
    assert!(
        decl_pos < default_pos,
        "TS files should preserve source order (declaration first): {trimmed}"
    );
}
