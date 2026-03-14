use super::*;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::{
    CallSignature, CallableShape, FunctionShape, ObjectFlags, ObjectShape, ParamInfo, PropertyInfo,
    SymbolRef, TupleElement, TypeId, TypeInterner,
};

// =============================================================================
// Helper
// =============================================================================

fn emit_dts(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut emitter = DeclarationEmitter::new(&parser.arena);
    emitter.emit(root)
}

fn emit_dts_with_binding(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    emitter.emit(root)
}

fn emit_dts_with_usage_analysis(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let current_arena = Arc::new(parser.arena.clone());

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    emitter.set_current_arena(current_arena, "test.ts".to_string());
    emitter.emit(root)
}

fn emit_js_dts(source: &str) -> String {
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut emitter = DeclarationEmitter::new(&parser.arena);
    emitter.emit(root)
}

fn emit_js_dts_with_usage_analysis(source: &str) -> String {
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let current_arena = Arc::new(parser.arena.clone());

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    emitter.set_current_arena(current_arena, "test.js".to_string());
    emitter.emit(root)
}

#[test]
fn test_same_file_symbol_module_path_is_none() {
    let source = r#"
namespace m1 {
    export class c {}
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let current_arena = Arc::new(parser.arena.clone());
    let arena_addr = Arc::as_ptr(&current_arena) as usize;
    let mut arena_to_path = FxHashMap::default();
    arena_to_path.insert(arena_addr, "test.ts".to_string());

    let interner = TypeInterner::new();
    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    emitter.set_current_arena(current_arena, "test.ts".to_string());
    emitter.set_arena_to_path(arena_to_path);

    let sym_id = binder
        .file_locals
        .get("m1")
        .expect("expected same-file namespace symbol");

    assert!(
        emitter.resolve_symbol_module_path(sym_id).is_none(),
        "Expected same-file symbol to have no module path"
    );
}

#[test]
fn test_same_file_generic_namespace_type_stays_unqualified() {
    let source = r#"
export namespace C {
    export class A<T> {}
    export class B {}
}

export const value = null as any;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let c_sym = binder
        .file_locals
        .get("C")
        .expect("missing namespace symbol");
    let c_symbol = binder.symbols.get(c_sym).expect("missing namespace data");
    let exports = c_symbol
        .exports
        .as_ref()
        .expect("expected namespace exports");
    let a_sym = exports.get("A").expect("missing class A symbol");
    let b_sym = exports.get("B").expect("missing class B symbol");

    let interner = TypeInterner::new();
    let a_def = tsz_solver::DefId(9101);
    let b_def = tsz_solver::DefId(9102);
    let value_type = interner.application(interner.lazy(a_def), vec![interner.lazy(b_def)]);

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.def_to_symbol.insert(a_def, a_sym);
    type_cache.def_to_symbol.insert(b_def, b_sym);

    let current_arena = Arc::new(parser.arena.clone());
    let arena_addr = Arc::as_ptr(&current_arena) as usize;
    let mut arena_to_path = FxHashMap::default();
    arena_to_path.insert(arena_addr, "test.ts".to_string());

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    emitter.set_current_arena(current_arena, "test.ts".to_string());
    emitter.set_arena_to_path(arena_to_path);
    let printed = emitter.print_type_id(value_type);

    assert!(
        printed == "C.A<C.B>",
        "Expected same-file generic type to stay local: {printed}"
    );
    assert!(
        !printed.contains("import(\"./test\").C.B"),
        "Did not expect same-file type references to be import-qualified: {printed}"
    );
}

#[test]
fn test_object_literal_enum_values_preserve_typeof_and_widen_members() {
    let output = emit_dts_with_binding(
        r#"
namespace m1 {
    export enum e {
        weekday,
        weekend,
        holiday,
    }
}

var d = {
    me: { en: m1.e },
    mh: m1.e.holiday,
};
"#,
    );

    assert!(
        output.contains("en: typeof m1.e;"),
        "Expected enum object value to emit typeof enum: {output}"
    );
    assert!(
        output.contains("mh: m1.e;"),
        "Expected enum member value to widen to enum type: {output}"
    );
    assert!(
        !output.contains("mh: m1.e.holiday;"),
        "Did not expect enum member literal to leak into anonymous object type: {output}"
    );
}

// =============================================================================
// 1. Simple Declarations
// =============================================================================

#[test]
fn test_function_declaration() {
    let source = "export function add(a: number, b: number): number { return a + b; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

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
        output.contains("export var K: {"),
        "Expected property-access CommonJS export to emit a synthetic declaration: {output}"
    );
    assert!(
        output.contains("new (): any;"),
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
        !output.contains("[~1]: {}"),
        "Did not expect non-emittable computed names to survive fallback object typing: {output}"
    );
    assert!(
        !output.contains("[!1]: {}"),
        "Did not expect non-emittable computed names to survive fallback object typing: {output}"
    );
}

#[test]
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
    static get OPTIONS(): any;
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
        output.contains("export declare var basePrototype: {\n    readonly primaryPath: any;\n};"),
        "Expected multi-line object literal accessor inference: {output}"
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

// =============================================================================
// 5. Type Formatting
// =============================================================================

#[test]
fn test_union_type_in_declaration() {
    let output = emit_dts("export type Result = string | number | boolean;");
    assert!(
        output.contains("string | number | boolean"),
        "Expected union type: {output}"
    );
}

#[test]
fn test_intersection_type_in_declaration() {
    let output = emit_dts("export type Combined = { a: number } & { b: string };");
    assert!(output.contains("&"), "Expected intersection type: {output}");
}

#[test]
fn test_function_type_in_declaration() {
    let output = emit_dts("export type Callback = (x: number, y: string) => void;");
    assert!(
        output.contains("(x: number, y: string) => void"),
        "Expected function type: {output}"
    );
}

#[test]
fn test_function_variable_type_preserves_inline_parameter_comments() {
    let output = emit_dts(
        r#"
const fooFunc = function (/** foo */ value: string): string {
    return value;
};
const lambdaFoo = (/** left */ left: number, /** right */ right: number): number => left + right;
"#,
    );

    assert!(
        output.contains("declare const fooFunc: (/** foo */ value: string) => string;"),
        "Expected function expression parameter comment to be preserved: {output}"
    );
    assert!(
        output.contains(
            "declare const lambdaFoo: (/** left */ left: number, /** right */ right: number) => number;"
        ),
        "Expected arrow function parameter comments to be preserved: {output}"
    );
}

#[test]
fn test_js_function_declaration_prefers_returned_callable_object_type() {
    let source = r#"
function test(fn) {
    const composed = function (...args) { };
    return composed;
}
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);
    let root_node = parser.arena.get(root).expect("missing root node");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file");
    let func_idx = source_file.statements.nodes[0];
    let func_node = parser.arena.get(func_idx).expect("missing function node");
    let func = parser
        .arena
        .get_function(func_node)
        .expect("missing function data");
    let body_node = parser.arena.get(func.body).expect("missing body node");
    let body = parser
        .arena
        .get_block(body_node)
        .expect("missing function body");
    let composed_stmt_idx = body.statements.nodes[0];
    let composed_decl = parser
        .arena
        .get(composed_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing composed declaration");
    let return_stmt_idx = body.statements.nodes[1];
    let return_stmt_node = parser
        .arena
        .get(return_stmt_idx)
        .expect("missing return node");
    let return_stmt = parser
        .arena
        .get_return_statement(return_stmt_node)
        .expect("missing return statement");

    let interner = TypeInterner::new();
    let fn_atom = interner.intern_string("fn");
    let args_atom = interner.intern_string("args");
    let name_atom = interner.intern_string("name");
    let any_array = interner.array(TypeId::ANY);
    let plain_return_type = interner.function(FunctionShape::new(
        vec![ParamInfo::rest(args_atom, any_array)],
        TypeId::VOID,
    ));
    let callable_return_type = interner.callable(CallableShape {
        call_signatures: vec![CallSignature::new(
            vec![ParamInfo::rest(args_atom, any_array)],
            TypeId::VOID,
        )],
        properties: vec![PropertyInfo::readonly(name_atom, TypeId::STRING)],
        ..Default::default()
    });
    let test_type = interner.function(FunctionShape::new(
        vec![ParamInfo::required(fn_atom, TypeId::ANY)],
        plain_return_type,
    ));

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(func_idx.0, test_type);
    type_cache.node_types.insert(func.name.0, test_type);
    type_cache
        .node_types
        .insert(composed_decl.name.0, callable_return_type);
    type_cache
        .node_types
        .insert(return_stmt.expression.0, callable_return_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare function test(fn: any): {"),
        "Expected JS function signature: {output}"
    );
    assert!(
        output.contains("(...args: any[]): void;"),
        "Expected callable return signature to be preserved: {output}"
    );
    assert!(
        output.contains("readonly name: string;"),
        "Expected returned callable property to be preserved: {output}"
    );
}

#[test]
fn test_any_dataview_new_expression_falls_back_to_generic_type() {
    let source = "const dataView = new DataView(new ArrayBuffer(80));";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(root_node) = parser.arena.get(root) else {
        panic!("missing root node");
    };
    let Some(source_file) = parser.arena.get_source_file(root_node) else {
        panic!("missing source file data");
    };
    let var_stmt_idx = source_file.statements.nodes[0];
    let var_decl = parser
        .arena
        .get(var_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing dataView declaration");

    let interner = TypeInterner::new();
    let mut type_cache = TypeCacheView::default();
    type_cache.node_types.insert(var_decl.name.0, TypeId::ANY);

    let binder = BinderState::new();
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare const dataView: DataView<ArrayBuffer>;"),
        "Expected DataView constructor fallback type: {output}"
    );
}

#[test]
fn test_static_method_property_access_emits_typeof() {
    let source = r#"
class C {
    static s1: number;
    static s2(b: number) {
        return C.s1 + b;
    }
}
var methodValue = C.s2;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let root_node = parser.arena.get(root).expect("missing root node");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file");
    let class_idx = source_file.statements.nodes[0];
    let class_decl = parser
        .arena
        .get(class_idx)
        .and_then(|node| parser.arena.get_class(node))
        .expect("missing class declaration");
    let var_stmt_idx = source_file.statements.nodes[1];
    let var_decl = parser
        .arena
        .get(var_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing variable declaration");
    let access = parser
        .arena
        .get(var_decl.initializer)
        .and_then(|node| parser.arena.get_access_expr(node))
        .expect("missing property access initializer");

    let interner = TypeInterner::new();
    let b_atom = interner.intern_string("b");
    let method_type = interner.function(FunctionShape::new(
        vec![ParamInfo::required(b_atom, TypeId::NUMBER)],
        TypeId::NUMBER,
    ));
    let constructor_type = interner.callable(CallableShape {
        call_signatures: Vec::new(),
        construct_signatures: vec![CallSignature::new(Vec::new(), TypeId::ANY)],
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: binder
            .get_node_symbol(class_decl.name)
            .or_else(|| binder.get_node_symbol(class_idx)),
        is_abstract: false,
    });

    let mut type_cache = TypeCacheView::default();
    type_cache.node_types.insert(var_decl.name.0, method_type);
    type_cache
        .node_types
        .insert(access.expression.0, constructor_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare var methodValue: typeof C.s2;"),
        "Expected static method property access to emit typeof: {output}"
    );
}

#[test]
fn test_const_call_initializer_does_not_collapse_to_literal_argument() {
    let source = r#"
type Box<T> = {
    get: () => T;
    set: (value: T) => void;
};
declare function box<T>(value: T): Box<T>;
const bn1 = box(0);
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let root_node = parser.arena.get(root).expect("missing root node");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file");
    let alias_idx = source_file.statements.nodes[0];
    let alias = parser
        .arena
        .get(alias_idx)
        .and_then(|node| parser.arena.get_type_alias(node))
        .expect("missing Box alias");
    let var_stmt_idx = source_file.statements.nodes[2];
    let var_decl = parser
        .arena
        .get(var_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing variable declaration");

    let interner = TypeInterner::new();
    let box_def = tsz_solver::DefId(9002);
    let box_number = interner.application(interner.lazy(box_def), vec![TypeId::NUMBER]);

    let alias_sym = binder
        .get_node_symbol(alias.name)
        .or_else(|| binder.get_node_symbol(alias_idx))
        .expect("missing Box symbol");
    let mut type_cache = TypeCacheView::default();
    type_cache.def_to_symbol.insert(box_def, alias_sym);
    type_cache.node_types.insert(var_decl.name.0, box_number);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare const bn1: Box<number>;"),
        "Expected const call initializer to preserve resolved type: {output}"
    );
    assert!(
        !output.contains("declare const bn1 = 0;"),
        "Did not expect const call initializer to collapse to its literal argument: {output}"
    );
}

#[test]
fn test_non_null_call_initializer_recovers_return_type() {
    let source = r#"
declare const fn: (() => string) | undefined;
const a = fn!();
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let root_node = parser.arena.get(root).expect("missing root node");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file");
    let fn_stmt_idx = source_file.statements.nodes[0];
    let fn_decl = parser
        .arena
        .get(fn_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing fn declaration");
    let a_stmt_idx = source_file.statements.nodes[1];
    let a_decl = parser
        .arena
        .get(a_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing a declaration");
    let call = parser
        .arena
        .get(a_decl.initializer)
        .and_then(|node| parser.arena.get_call_expr(node))
        .expect("missing call initializer");
    let non_null = parser
        .arena
        .get(call.expression)
        .and_then(|node| parser.arena.get_unary_expr_ex(node))
        .expect("missing non-null callee");
    let interner = TypeInterner::new();
    let callable = interner.function(FunctionShape::new(Vec::new(), TypeId::STRING));

    let mut type_cache = TypeCacheView::default();
    type_cache.node_types.insert(fn_decl.name.0, callable);
    type_cache
        .node_types
        .insert(non_null.expression.0, callable);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare const a: string;"),
        "Expected non-null call initializer to recover the inner callable return type: {output}"
    );
}

#[test]
fn test_dataview_new_expression_falls_back_without_type_cache() {
    let output = emit_dts("const dataView = new DataView(new ArrayBuffer(80));");
    assert!(
        output.contains("declare const dataView: DataView<ArrayBuffer>;"),
        "Expected DataView constructor fallback without type cache: {output}"
    );
}

#[test]
fn test_array_type_in_declaration() {
    let output = emit_dts("export type Numbers = number[];");
    assert!(output.contains("number[]"), "Expected array type: {output}");
}

#[test]
fn test_tuple_type_in_declaration() {
    let output = emit_dts("export type Pair = [string, number];");
    assert!(
        output.contains("[string, number]"),
        "Expected tuple type: {output}"
    );
}

#[test]
fn test_conditional_type_in_declaration() {
    let output = emit_dts("export type IsString<T> = T extends string ? true : false;");
    assert!(
        output.contains("T extends string ? true : false"),
        "Expected conditional type: {output}"
    );
}

#[test]
fn test_mapped_type_in_declaration() {
    let output = emit_dts("export type Readonly<T> = { readonly [K in keyof T]: T[K] };");
    assert!(
        output.contains("readonly"),
        "Expected mapped type with readonly: {output}"
    );
    assert!(
        output.contains("keyof T"),
        "Expected keyof in mapped type: {output}"
    );
}

#[test]
fn test_indexed_access_type() {
    let output = emit_dts("export type Name = Person['name'];");
    assert!(
        output.contains("Person["),
        "Expected indexed access type: {output}"
    );
}

#[test]
fn test_typeof_type() {
    let output = emit_dts("declare const x: number;\nexport type T = typeof x;");
    assert!(
        output.contains("typeof x"),
        "Expected typeof type: {output}"
    );
}

// =============================================================================
// 6. Generic Declarations
// =============================================================================

#[test]
fn test_generic_function() {
    let output = emit_dts("export function identity<T>(x: T): T { return x; }");
    assert!(
        output.contains("<T>"),
        "Expected generic type parameter: {output}"
    );
    assert!(
        output.contains("x: T"),
        "Expected parameter with generic type: {output}"
    );
    assert!(
        output.contains("): T;"),
        "Expected return type with generic: {output}"
    );
}

#[test]
fn test_generic_interface_with_constraint() {
    let output = emit_dts(
        r#"
    export interface Container<T extends object> {
        value: T;
    }
    "#,
    );
    assert!(
        output.contains("<T extends object>"),
        "Expected generic type parameter with constraint: {output}"
    );
    assert!(
        output.contains("value: T;"),
        "Expected member with generic type: {output}"
    );
}

#[test]
fn test_generic_class_with_default() {
    let output = emit_dts(
        r#"
    export class Box<T = string> {
        content: T;
        constructor(value: T) { this.content = value; }
    }
    "#,
    );
    assert!(
        output.contains("<T = string>"),
        "Expected generic type parameter with default: {output}"
    );
}

#[test]
fn test_multiple_type_parameters() {
    let output = emit_dts(
        "export function map<T, U>(arr: T[], fn: (x: T) => U): U[] { return arr.map(fn); }",
    );
    assert!(
        output.contains("<T, U>"),
        "Expected multiple type parameters: {output}"
    );
}

// =============================================================================
// 7. Ambient / Declare Declarations
// =============================================================================

#[test]
fn test_declare_class_passthrough() {
    let output = emit_dts(
        r#"
    declare class Foo {
        bar(): void;
    }
    "#,
    );
    assert!(
        output.contains("declare class Foo"),
        "Expected declare class: {output}"
    );
    assert!(
        output.contains("bar(): void;"),
        "Expected method signature: {output}"
    );
}

#[test]
fn test_declare_function_passthrough() {
    let output = emit_dts("declare function greet(name: string): void;");
    assert!(
        output.contains("declare function greet(name: string): void;"),
        "Expected declare function: {output}"
    );
}

#[test]
fn test_declare_var_passthrough() {
    let output = emit_dts("declare var globalName: string;");
    assert!(
        output.contains("declare var globalName: string;"),
        "Expected declare var: {output}"
    );
}

// =============================================================================
// 8. Module / Namespace Declarations
// =============================================================================

#[test]
fn test_namespace_declaration() {
    let output = emit_dts(
        r#"
    export declare namespace MyLib {
        function create(): void;
        class Widget {
            name: string;
        }
    }
    "#,
    );
    assert!(
        output.contains("export declare namespace MyLib"),
        "Expected namespace declaration: {output}"
    );
    assert!(
        output.contains("function create(): void;"),
        "Expected function in namespace: {output}"
    );
    assert!(
        output.contains("class Widget"),
        "Expected class in namespace: {output}"
    );
}

#[test]
fn test_nested_namespace() {
    let output = emit_dts(
        r#"
    export declare namespace Outer {
        namespace Inner {
            const value: number;
        }
    }
    "#,
    );
    assert!(
        output.contains("namespace Outer"),
        "Expected outer namespace: {output}"
    );
    assert!(
        output.contains("namespace Inner"),
        "Expected inner namespace: {output}"
    );
}

// =============================================================================
// 9. Enum Declarations
// =============================================================================

#[test]
fn test_regular_enum() {
    let output = emit_dts(
        r#"
    export enum Color {
        Red,
        Green,
        Blue
    }
    "#,
    );
    assert!(
        output.contains("export declare enum Color"),
        "Expected exported declare enum: {output}"
    );
    assert!(output.contains("Red"), "Expected Red member: {output}");
    assert!(output.contains("Green"), "Expected Green member: {output}");
    assert!(output.contains("Blue"), "Expected Blue member: {output}");
}

#[test]
fn test_const_enum() {
    let output = emit_dts(
        r#"
    export const enum Direction {
        Up = 0,
        Down = 1,
        Left = 2,
        Right = 3
    }
    "#,
    );
    assert!(
        output.contains("export declare const enum Direction"),
        "Expected exported declare const enum: {output}"
    );
    assert!(output.contains("Up = 0"), "Expected Up = 0: {output}");
    assert!(output.contains("Right = 3"), "Expected Right = 3: {output}");
}

#[test]
fn test_invalid_const_enum_object_index_access_emits_any() {
    let output = emit_dts_with_binding(
        r#"
const enum G {
    A = 1,
    B = 2,
}
let z1 = G[G.A];
"#,
    );

    assert!(
        output.contains("declare let z1: any;"),
        "Expected invalid const enum object index access to emit any: {output}"
    );
}

#[test]
fn test_string_enum() {
    let output = emit_dts(
        r#"
    export enum Status {
        Active = "active",
        Inactive = "inactive"
    }
    "#,
    );
    assert!(
        output.contains("Active = \"active\""),
        "Expected string enum value: {output}"
    );
    assert!(
        output.contains("Inactive = \"inactive\""),
        "Expected string enum value: {output}"
    );
}

#[test]
fn test_enum_auto_increment() {
    let output = emit_dts(
        r#"
    export enum Seq {
        A = 10,
        B,
        C
    }
    "#,
    );
    assert!(output.contains("A = 10"), "Expected A = 10: {output}");
    assert!(
        output.contains("B = 11"),
        "Expected B = 11 (auto-increment): {output}"
    );
    assert!(
        output.contains("C = 12"),
        "Expected C = 12 (auto-increment): {output}"
    );
}

// =============================================================================
// 10. Class Advanced Features
// =============================================================================

#[test]
fn test_abstract_class() {
    let output = emit_dts(
        r#"
    export abstract class Shape {
        abstract area(): number;
        name: string;
        constructor(name: string) { this.name = name; }
    }
    "#,
    );
    assert!(
        output.contains("export declare abstract class Shape"),
        "Expected abstract class: {output}"
    );
    assert!(
        output.contains("abstract area(): number;"),
        "Expected abstract method: {output}"
    );
}

#[test]
fn test_class_with_heritage() {
    let output = emit_dts(
        r#"
    export class Dog extends Animal implements Pet {
        bark(): void {}
    }
    "#,
    );
    assert!(
        output.contains("extends Animal"),
        "Expected extends clause: {output}"
    );
    assert!(
        output.contains("implements Pet"),
        "Expected implements clause: {output}"
    );
}

#[test]
fn test_constructor_declaration() {
    let output = emit_dts(
        r#"
    export class Point {
        x: number;
        y: number;
        constructor(x: number, y: number) {
            this.x = x;
            this.y = y;
        }
    }
    "#,
    );
    assert!(
        output.contains("constructor(x: number, y: number);"),
        "Expected constructor in .d.ts: {output}"
    );
}

#[test]
fn test_parameter_properties() {
    let output = emit_dts(
        r#"
    export class Point {
        constructor(public x: number, protected y: number, private z: number) {}
    }
    "#,
    );
    // Parameter properties should be emitted as class properties
    assert!(
        output.contains("x: number;"),
        "Expected public parameter property as class property: {output}"
    );
    assert!(
        output.contains("protected y: number;"),
        "Expected protected parameter property: {output}"
    );
    assert!(
        output.contains("private z;"),
        "Expected private parameter property (without type): {output}"
    );
}

#[test]
fn test_optional_parameter_property_emits_undefined_in_constructor_and_property() {
    let output = emit_dts(
        r#"
    export class Point {
        constructor(public x?: string) {}
    }
    "#,
    );

    assert!(
        output.contains("x?: string | undefined;"),
        "Expected optional parameter property to include undefined in property type: {output}"
    );
    assert!(
        output.contains("constructor(x?: string | undefined);"),
        "Expected optional parameter property to include undefined in constructor type: {output}"
    );
}

#[test]
fn test_parameter_property_initializer_infers_property_type() {
    let output = emit_dts(
        r#"
    export class Point {
        constructor(public x = "hello") {}
    }
    "#,
    );

    assert!(
        output.contains("x: string;"),
        "Expected initializer-backed parameter property to infer a property type: {output}"
    );
    assert!(
        output.contains("constructor(x?: string);"),
        "Expected initializer-backed parameter property constructor to stay optional: {output}"
    );
}

#[test]
fn test_getter_and_setter() {
    let output = emit_dts(
        r#"
    export class Foo {
        get value(): number { return 42; }
        set value(v: number) {}
    }
    "#,
    );
    assert!(
        output.contains("get value(): number;"),
        "Expected getter declaration: {output}"
    );
    assert!(
        output.contains("set value(v: number);"),
        "Expected setter declaration: {output}"
    );
}

#[test]
fn test_static_member() {
    let output = emit_dts(
        r#"
    export class Singleton {
        static instance: Singleton;
        static create(): Singleton { return new Singleton(); }
    }
    "#,
    );
    assert!(
        output.contains("static instance"),
        "Expected static property: {output}"
    );
    assert!(
        output.contains("static create"),
        "Expected static method: {output}"
    );
}

#[test]
fn test_readonly_property() {
    let output = emit_dts(
        r#"
    export class Config {
        readonly name: string;
        constructor(name: string) { this.name = name; }
    }
    "#,
    );
    assert!(
        output.contains("readonly name: string;"),
        "Expected readonly property: {output}"
    );
}

#[test]
fn test_index_signature_in_class() {
    let output = emit_dts(
        r#"
    export class Dict {
        [key: string]: any;
    }
    "#,
    );
    assert!(
        output.contains("[key: string]: any;"),
        "Expected index signature in class: {output}"
    );
}

#[test]
fn test_index_signature_in_interface() {
    let output = emit_dts(
        r#"
    export interface StringMap {
        [key: string]: string;
    }
    "#,
    );
    assert!(
        output.contains("[key: string]: string;"),
        "Expected index signature in interface: {output}"
    );
}

#[test]
fn test_optional_property_in_interface() {
    let output = emit_dts(
        r#"
    export interface Config {
        name: string;
        debug?: boolean;
    }
    "#,
    );
    assert!(
        output.contains("debug?: boolean;"),
        "Expected optional property: {output}"
    );
}

#[test]
fn test_optional_method_in_interface() {
    let output = emit_dts(
        r#"
    export interface Plugin {
        init?(): void;
    }
    "#,
    );
    assert!(
        output.contains("init?(): void;"),
        "Expected optional method: {output}"
    );
}

#[test]
fn test_optional_computed_method_in_class_emits_optional_property_function_type() {
    let output = emit_dts(
        r#"
    export const dataSomething: `data-${string}` = "data-x" as `data-${string}`;
    export class WithData {
        [dataSomething]?(): string {
            return "something";
        }
    }
    "#,
    );
    // tsc emits optional methods with method syntax: [key]?(): type;
    // The `?` mark conveys optionality without needing `| undefined`.
    assert!(
        output.contains("[dataSomething]?(): string;"),
        "Expected optional computed method to use method syntax with ?: {output}"
    );
}

#[test]
fn test_static_computed_methods_emit_body_inferred_return_types() {
    let output = emit_dts(
        r#"
    export declare const f1: string;
    export declare const f2: string;

    export class Holder {
        static [f1]() {
            return { static: true };
        }
        static [f2]() {
            return { static: "sometimes" };
        }
    }

    export const staticLookup = Holder["x"];
    "#,
    );
    // tsc emits computed methods as method signatures, not property signatures.
    assert!(
        output.contains("static [f1](): {")
            && output.contains("static: boolean;")
            && output.contains("static [f2](): {")
            && output.contains("static: string;"),
        "Expected static computed methods to use method syntax with body-inferred return types: {output}"
    );
}

// =============================================================================
// 11. Function Overloads
// =============================================================================

#[test]
fn test_function_overloads_emit_only_signatures() {
    let output = emit_dts(
        r#"
    export function parse(input: string): number;
    export function parse(input: number): string;
    export function parse(input: any): any { return input; }
    "#,
    );
    // Both overload signatures should be emitted
    assert!(
        output.contains("export declare function parse(input: string): number;"),
        "Expected first overload: {output}"
    );
    assert!(
        output.contains("export declare function parse(input: number): string;"),
        "Expected second overload: {output}"
    );
    // Implementation should NOT be emitted
    assert!(
        !output.contains("input: any): any;"),
        "Implementation signature should not appear: {output}"
    );
}

// =============================================================================
// 12. Interface Heritage
// =============================================================================

#[test]
fn test_interface_extends() {
    let output = emit_dts(
        r#"
    export interface Animal {
        name: string;
    }
    export interface Dog extends Animal {
        breed: string;
    }
    "#,
    );
    assert!(
        output.contains("interface Dog extends Animal"),
        "Expected interface extends: {output}"
    );
}

// =============================================================================
// 13. Private Identifier (#private)
// =============================================================================

#[test]
fn test_private_identifier_emits_private_marker() {
    let output = emit_dts(
        r#"
    export class Foo {
        #secret: number;
        getValue(): number { return this.#secret; }
    }
    "#,
    );
    // Private identifiers should produce `#private;`
    assert!(
        output.contains("#private;"),
        "Expected #private marker for private identifiers: {output}"
    );
    // The actual #secret name should NOT appear
    assert!(
        !output.contains("#secret"),
        "#secret should not appear in .d.ts: {output}"
    );
}

// =============================================================================
// 14. Numeric Literal Normalization
// =============================================================================

#[test]
fn test_normalize_numeric_literal_unchanged() {
    assert_eq!(DeclarationEmitter::normalize_numeric_literal("42"), "42");
    assert_eq!(
        DeclarationEmitter::normalize_numeric_literal("3.14"),
        "3.14"
    );
    assert_eq!(DeclarationEmitter::normalize_numeric_literal("0"), "0");
}

#[test]
fn test_normalize_numeric_literal_large_integer() {
    // Very large integers should be normalized through f64 round-trip
    let result = DeclarationEmitter::normalize_numeric_literal(
        "123456789123456789123456789123456789123456789123456789",
    );
    assert!(
        result.contains("e+"),
        "Expected scientific notation for very large number: {result}"
    );
}

// =============================================================================
// 15. Format JS Number
// =============================================================================

#[test]
fn test_format_js_number_infinity() {
    assert_eq!(
        DeclarationEmitter::format_js_number(f64::INFINITY),
        "Infinity"
    );
    assert_eq!(
        DeclarationEmitter::format_js_number(f64::NEG_INFINITY),
        "-Infinity"
    );
}

#[test]
fn test_format_js_number_nan() {
    assert_eq!(DeclarationEmitter::format_js_number(f64::NAN), "NaN");
}

#[test]
fn test_format_js_number_integers() {
    assert_eq!(DeclarationEmitter::format_js_number(0.0), "0");
    assert_eq!(DeclarationEmitter::format_js_number(42.0), "42");
    assert_eq!(DeclarationEmitter::format_js_number(-1.0), "-1");
}

#[test]
fn test_format_js_number_floats() {
    assert_eq!(DeclarationEmitter::format_js_number(3.15), "3.15");
    assert_eq!(DeclarationEmitter::format_js_number(0.5), "0.5");
}

// =============================================================================
// 16. Rest Parameters
// =============================================================================

#[test]
fn test_rest_parameter_in_function() {
    let output = emit_dts("export function sum(...nums: number[]): number { return 0; }");
    assert!(
        output.contains("...nums: number[]"),
        "Expected rest parameter: {output}"
    );
}

// =============================================================================
// 17. Call / Construct Signatures in Interfaces
// =============================================================================

#[test]
fn test_call_signature_in_interface() {
    let output = emit_dts(
        r#"
    export interface Callable {
        (x: number): string;
    }
    "#,
    );
    assert!(
        output.contains("(x: number): string;"),
        "Expected call signature: {output}"
    );
}

#[test]
fn test_construct_signature_in_interface() {
    let output = emit_dts(
        r#"
    export interface Constructable {
        new (name: string): object;
    }
    "#,
    );
    assert!(
        output.contains("new (name: string): object;"),
        "Expected construct signature: {output}"
    );
}

// =============================================================================
// 18. Type Predicate (type guard)
// =============================================================================

#[test]
fn test_type_predicate_in_function() {
    let output = emit_dts(
        r#"
    export function isString(x: unknown): x is string {
        return typeof x === "string";
    }
    "#,
    );
    assert!(
        output.contains("x is string"),
        "Expected type predicate: {output}"
    );
}

// =============================================================================
// 19. Default Parameter Values (stripped)
// =============================================================================

#[test]
fn test_default_parameter_values_omitted() {
    let output = emit_dts(
        r#"
    export function greet(name: string = "world"): void {}
    "#,
    );
    // Default values should be stripped; parameter should remain with its type
    assert!(
        output.contains("name"),
        "Expected parameter name preserved: {output}"
    );
    // The default value itself should not appear in the .d.ts
    assert!(
        !output.contains("\"world\""),
        "Default value should be stripped from .d.ts: {output}"
    );
}

// =============================================================================
// 20. Using declaration emits as const
// =============================================================================

#[test]
fn test_using_declaration_emits_const() {
    let output = emit_dts(r#"export using x: Disposable = getResource();"#);
    // `using` declarations emit as `const` in .d.ts
    assert!(
        output.contains("const x"),
        "Expected using declaration to emit as const: {output}"
    );
}

// =============================================================================
// 21. Void-returning function body inference
// =============================================================================

#[test]
fn test_void_body_function_infers_void_return() {
    let output = emit_dts(
        r#"
    export function doNothing() {
        console.log("hi");
    }
    "#,
    );
    assert!(
        output.contains("void"),
        "Expected void return type for function with no return: {output}"
    );
}

// =============================================================================
// 22. Side-effect imports preserved
// =============================================================================

#[test]
fn test_side_effect_import_preserved() {
    let output = emit_dts(r#"import "./polyfill";"#);
    assert!(
        output.contains("import \"./polyfill\""),
        "Expected side-effect import to be preserved: {output}"
    );
}

// =============================================================================
// 23. Literal type aliases
// =============================================================================

#[test]
fn test_literal_type_alias() {
    let output = emit_dts("export type Direction = 'up' | 'down' | 'left' | 'right';");
    assert!(
        output.contains("'up'") || output.contains("\"up\""),
        "Expected string literal type: {output}"
    );
}

// =============================================================================
// 24. Keyof type
// =============================================================================

#[test]
fn test_keyof_type() {
    let output = emit_dts("export type Keys<T> = keyof T;");
    assert!(output.contains("keyof T"), "Expected keyof type: {output}");
}

// =============================================================================
// 25. Type operator (readonly arrays)
// =============================================================================

#[test]
fn test_readonly_array_type() {
    let output = emit_dts("export type ReadonlyArr = readonly number[];");
    assert!(
        output.contains("readonly number[]"),
        "Expected readonly array type: {output}"
    );
}

// =============================================================================
// 26. Parenthesized type
// =============================================================================

#[test]
fn test_parenthesized_function_type_in_array() {
    let output = emit_dts("export type FnArray = ((x: number) => void)[];");
    assert!(
        output.contains("((x: number) => void)[]"),
        "Expected parenthesized function type in array: {output}"
    );
}

// =============================================================================
// 27. Computed property names
// =============================================================================

#[test]
fn test_computed_symbol_property() {
    let output = emit_dts(
        r#"
    export interface Iterable {
        [Symbol.iterator](): Iterator<any>;
    }
    "#,
    );
    assert!(
        output.contains("[Symbol.iterator]"),
        "Expected computed Symbol property: {output}"
    );
}

// =============================================================================
// 28. Export assignment (export =)
// =============================================================================

#[test]
fn test_export_equals() {
    let output = emit_dts(
        r#"
    declare const myLib: { version: string };
    export = myLib;
    "#,
    );
    assert!(
        output.contains("export = myLib;"),
        "Expected export = : {output}"
    );
}

#[test]
fn test_export_equals_import_equals_keeps_namespace_dependency() {
    let output = emit_dts_with_usage_analysis(
        r#"
    namespace m3 {
        export namespace m2 {
            export interface connectModule {
                (res, req, next): void;
            }
            export interface connectExport {
                use: (mod: connectModule) => connectExport;
                listen: (port: number) => void;
            }
        }

        export var server: {
            (): m2.connectExport;
            test1: m2.connectModule;
            test2(): m2.connectModule;
        };
    }

    import m = m3;
    export = m;
    "#,
    );

    let namespace_pos = output
        .find("declare namespace m3")
        .expect("Expected namespace dependency to be preserved");
    let import_pos = output
        .find("import m = m3;")
        .expect("Expected import equals alias to be emitted");
    let export_pos = output
        .find("export = m;")
        .expect("Expected export assignment to be emitted");

    assert!(
        namespace_pos < import_pos && import_pos < export_pos,
        "Expected namespace, import alias, and export assignment to preserve source order: {output}"
    );
}

#[test]
fn test_export_equals_import_equals_chain_keeps_namespace_dependency() {
    let output = emit_dts_with_usage_analysis(
        r#"
    namespace m {
        export namespace c {
            export class c {
            }
        }
    }

    import a = m.c;
    import b = a;
    export = b;
    "#,
    );

    let namespace_pos = output
        .find("declare namespace m")
        .expect("Expected namespace dependency to be preserved");
    let first_import_pos = output
        .find("import a = m.c;")
        .expect("Expected first import equals alias to be emitted");
    let second_import_pos = output
        .find("import b = a;")
        .expect("Expected chained import equals alias to be emitted");
    let export_pos = output
        .find("export = b;")
        .expect("Expected export assignment to be emitted");

    assert!(
        namespace_pos < first_import_pos
            && first_import_pos < second_import_pos
            && second_import_pos < export_pos,
        "Expected namespace, import chain, and export assignment to preserve source order: {output}"
    );
}

#[test]
fn test_import_type_with_resolution_mode_attributes_is_preserved() {
    let output = emit_dts_with_usage_analysis(
        r#"
    import type { RequireInterface } from "pkg" with { "resolution-mode": "require" };
    import { type RequireInterface as Req } from "pkg" with { "resolution-mode": "require" };

    export interface LocalInterface extends RequireInterface {}
    export interface Loc extends Req {}
    "#,
    );

    assert!(
        output.contains(
            r#"import type { RequireInterface } from "pkg" with { "resolution-mode": "require" };"#
        ),
        "Expected type-only import attributes to be preserved: {output}"
    );
    assert!(
        output.contains(
            r#"import { type RequireInterface as Req } from "pkg" with { "resolution-mode": "require" };"#
        ),
        "Expected named import attributes to be preserved: {output}"
    );
}

#[test]
fn test_import_type_alias_is_preserved_with_usage_analysis() {
    let output = emit_dts_with_usage_analysis(
        r#"
    import { type RequireInterface as Req } from "pkg";

    export interface Loc extends Req {}
    "#,
    );

    assert!(
        output.contains(r#"import { type RequireInterface as Req } from "pkg";"#),
        "Expected aliased type import to be preserved: {output}"
    );
}

#[test]
fn test_namespace_import_type_is_preserved_with_usage_analysis() {
    let source = r#"
    import * as ns from "pkg";
    export const value = ns;
    "#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let root_node = parser.arena.get(root).expect("missing root node");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file");
    let var_stmt = source_file
        .statements
        .nodes
        .iter()
        .find_map(|&stmt_idx| {
            let stmt_node = parser.arena.get(stmt_idx)?;
            if let Some(var_stmt) = parser.arena.get_variable(stmt_node) {
                return Some(var_stmt);
            }
            let export = parser.arena.get_export_decl(stmt_node)?;
            let clause_node = parser.arena.get(export.export_clause)?;
            parser.arena.get_variable(clause_node)
        })
        .expect("missing variable statement");
    let decl_list_idx = var_stmt.declarations.nodes[0];
    let decl_list = parser
        .arena
        .get(decl_list_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .expect("missing declaration list");
    let decl_idx = decl_list.declarations.nodes[0];
    let decl = parser
        .arena
        .get(decl_idx)
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing declaration");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let ns_sym_id = binder
        .file_locals
        .get("ns")
        .expect("expected namespace import symbol");

    let interner = TypeInterner::new();
    let namespace_type = interner.module_namespace(SymbolRef(ns_sym_id.0));

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(decl.name.0, namespace_type);

    let current_arena = Arc::new(parser.arena.clone());
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    emitter.set_current_arena(current_arena, "test.ts".to_string());
    let output = emitter.emit(root);

    assert!(
        output.contains(r#"import * as ns from "pkg";"#),
        "Expected namespace import to be preserved: {output}"
    );
    assert!(
        output.contains("export declare const value: typeof ns;"),
        "Expected exported value to use the namespace import alias type: {output}"
    );
}

#[test]
fn test_exported_namespace_import_initializer_preserves_typeof_alias() {
    let output = emit_dts_with_usage_analysis(
        r#"
    import * as ns from "pkg";
    export const value = ns;
    "#,
    );

    assert!(
        output.contains(r#"import * as ns from "pkg";"#),
        "Expected namespace import to survive usage analysis: {output}"
    );
    assert!(
        output.contains("export declare const value: typeof ns;"),
        "Expected exported namespace import initializer to emit typeof alias: {output}"
    );
}

#[test]
fn test_call_expression_recovers_return_type_from_callee_type() {
    let source = r#"
    export const a = helper.x();
    "#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let root_node = parser.arena.get(root).expect("missing root node");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file");
    let var_stmt = source_file
        .statements
        .nodes
        .iter()
        .find_map(|&stmt_idx| {
            let stmt_node = parser.arena.get(stmt_idx)?;
            if let Some(var_stmt) = parser.arena.get_variable(stmt_node) {
                return Some(var_stmt);
            }
            let export = parser.arena.get_export_decl(stmt_node)?;
            let clause_node = parser.arena.get(export.export_clause)?;
            parser.arena.get_variable(clause_node)
        })
        .expect("missing variable statement");
    let decl_list_idx = var_stmt.declarations.nodes[0];
    let decl_list = parser
        .arena
        .get(decl_list_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .expect("missing declaration list");
    let decl_idx = decl_list.declarations.nodes[0];
    let decl = parser
        .arena
        .get(decl_idx)
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing declaration");
    let call = parser
        .arena
        .get(decl.initializer)
        .and_then(|node| parser.arena.get_call_expr(node))
        .expect("missing call expression");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let callee_type = interner.function(FunctionShape::new(Vec::new(), TypeId::STRING));

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(call.expression.0, callee_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("export declare const a: string;"),
        "Expected call expression to recover return type from callee type: {output}"
    );
}

#[test]
fn test_export_type_with_resolution_mode_attributes_is_preserved() {
    let output = emit_dts_with_usage_analysis(
        r#"
    export type { RequireInterface } from "pkg" with { "resolution-mode": "require" };
    "#,
    );

    assert!(
        output.contains(
            r#"export type { RequireInterface } from "pkg" with { "resolution-mode": "require" };"#
        ),
        "Expected export type attributes to be preserved: {output}"
    );
}

#[test]
fn test_asserted_import_type_with_resolution_mode_attributes_is_preserved() {
    let output = emit_dts(
        r#"
    export type LocalInterface = import("pkg", { with: {"resolution-mode": "require"} }).RequireInterface;
    export const value = (null as any as import("pkg", { with: {"resolution-mode": "require"} }).RequireInterface);
    "#,
    );

    assert!(
        output.contains(
            r#"export type LocalInterface = import("pkg", { with: { "resolution-mode": "require" } }).RequireInterface;"#
        ),
        "Expected import type attributes to be formatted canonically in type aliases: {output}"
    );
    assert!(
        output.contains(
            r#"export declare const value: import("pkg", { with: { "resolution-mode": "require" } }).RequireInterface;"#
        ),
        "Expected asserted import type with attributes to be preserved on exported values: {output}"
    );
}

#[test]
fn test_invalid_resolution_mode_attribute_is_dropped_and_unused_mixed_import_is_elided() {
    let output = emit_dts_with_usage_analysis(
        r#"
    import type { RequireInterface } from "pkg" with { "resolution-mode": "foobar" };
    import { ImportInterface } from "pkg" with { "resolution-mode": "import" };
    import { type RequireInterface as Req, RequireInterface as Req2 } from "pkg" with { "resolution-mode": "require" };

    export interface LocalInterface extends RequireInterface, ImportInterface {}
    "#,
    );

    assert!(
        output.contains(r#"import type { RequireInterface } from "pkg";"#),
        "Expected invalid resolution-mode attribute to be dropped: {output}"
    );
    assert!(
        output.contains(
            r#"import { ImportInterface } from "pkg" with { "resolution-mode": "import" };"#
        ),
        "Expected valid resolution-mode attribute to be preserved: {output}"
    );
    assert!(
        !output.contains("Req2"),
        "Expected unused mixed import bindings to be elided: {output}"
    );
}

// =============================================================================
// 29. Namespace export as
// =============================================================================

#[test]
fn test_star_export_as_namespace() {
    let output = emit_dts(r#"export * as utils from "./utils";"#);
    assert!(
        output.contains("export * as utils from"),
        "Expected namespace re-export: {output}"
    );
}

// =============================================================================
// 30. Asserts modifier in type predicate
// =============================================================================

#[test]
fn test_assertion_function() {
    let output = emit_dts(
        r#"
    export function assertDefined(val: unknown): asserts val {
        if (val == null) throw new Error();
    }
    "#,
    );
    assert!(
        output.contains("asserts val"),
        "Expected asserts modifier: {output}"
    );
}

// =============================================================================
// 31. Multiple variable declarations on one line
// =============================================================================

#[test]
fn test_multiple_variable_declarators() {
    let output = emit_dts("export var x: number, y: string;");
    assert!(
        output.contains("x: number"),
        "Expected first variable: {output}"
    );
    assert!(
        output.contains("y: string"),
        "Expected second variable: {output}"
    );
}

#[test]
fn test_destructuring_variable_declaration_groups_typed_bindings() {
    let source = r#"var [x, y] = [1, "hello"];"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let root_node = parser.arena.get(root).expect("missing root node");
    let stmt_idx = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file")
        .statements
        .nodes[0];
    let stmt = parser
        .arena
        .get(stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .expect("missing variable statement");
    let decl_list = parser
        .arena
        .get(stmt.declarations.nodes[0])
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
    let tuple_type = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(decl.initializer.0, tuple_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare var x: number, y: string;"),
        "Expected destructured bindings to emit in one typed declaration: {output}"
    );
}

#[test]
fn test_destructuring_parameter_properties_emit_individual_class_properties() {
    let source = "class C { constructor(public [x, y]: [string, number]) {} }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let root_node = parser.arena.get(root).expect("missing root node");
    let stmt_idx = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file")
        .statements
        .nodes[0];
    let class_decl = parser
        .arena
        .get(stmt_idx)
        .and_then(|node| parser.arena.get_class(node))
        .expect("missing class declaration");
    let ctor_idx = class_decl.members.nodes[0];
    let ctor = parser
        .arena
        .get(ctor_idx)
        .and_then(|node| parser.arena.get_constructor(node))
        .expect("missing constructor");
    let param_idx = ctor.parameters.nodes[0];
    let param = parser
        .arena
        .get(param_idx)
        .and_then(|node| parser.arena.get_parameter(node))
        .expect("missing parameter");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let tuple_type = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache
        .node_types
        .insert(param.type_annotation.0, tuple_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("x: string;"),
        "Expected first destructured parameter property to be emitted: {output}"
    );
    assert!(
        output.contains("y: number;"),
        "Expected second destructured parameter property to be emitted: {output}"
    );
    assert!(
        !output.contains("[x, y]: [string, number];"),
        "Did not expect destructuring pattern to be emitted as a property name: {output}"
    );
}
