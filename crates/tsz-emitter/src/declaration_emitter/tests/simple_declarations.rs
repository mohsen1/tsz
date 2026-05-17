use super::*;
fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

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
fn test_invalid_ambient_style_getter_defaults_to_any() {
    let source = r#"
export class C {
    get value()
}
"#;
    let output = emit_dts_with_usage_analysis(source);

    assert!(
        output.contains("get value(): any;"),
        "Expected no-body getter recovery to emit any: {output}"
    );
}

#[test]
fn test_legacy_index_signature_defaults_to_any() {
    let source = r#"
export interface I {
    [p];
    [p2: string, p3: number];
}
"#;
    let output = emit_dts_with_usage_analysis(source);

    assert!(
        output.contains("[p]: any;"),
        "Expected untyped index signature to emit any result: {output}"
    );
    assert!(
        output.contains("[p2: string, p3: number]: any;"),
        "Expected legacy index signature parameters to be preserved: {output}"
    );
}

#[test]
fn test_index_signature_preserves_inline_parameter_comment() {
    let source = r#"
export interface I {
    /** indexer */
    [/** key param */ key: string]: any;
}
"#;
    let output = emit_dts_with_usage_analysis(source);

    assert!(
        output.contains("[/** key param */ key: string]: any;"),
        "Expected index signature parameter comment to be preserved: {output}"
    );
}

#[test]
fn test_non_exported_namespace_hidden_inside_non_ambient_namespace() {
    let source = r#"
export namespace Outer {
    namespace Hidden {
        export var x;
    }
    export declare namespace Ambient {
        var y;
    }
    export var z;
}
"#;
    let output = emit_dts_with_usage_analysis(source);

    assert!(
        !output.contains("namespace Hidden"),
        "Expected hidden non-exported namespace to be elided: {output}"
    );
    assert!(
        output.contains("namespace Ambient {\n        var y: any;"),
        "Expected declared nested namespace body to remain ambient: {output}"
    );
    assert!(
        output.contains("var z: any;"),
        "Expected exported namespace member to be preserved: {output}"
    );
}

#[test]
fn test_throw_only_unannotated_returns_void() {
    let source = r#"
export function f() {
    throw new Error();
}
export class C {
    m() {
        throw new Error();
    }
}
"#;
    let output = emit_dts_with_usage_analysis(source);

    assert!(
        output.contains("export declare function f(): void;"),
        "Expected throw-only function to emit void: {output}"
    );
    assert!(
        output.contains("m(): void;"),
        "Expected throw-only method to emit void: {output}"
    );
}

#[test]
fn test_defaulted_boolean_param_false_narrowing_return_type() {
    let source = r#"
function removeUndefinedButNotFalse(x = true) {
    if (x === false) {
        return x;
    }
}

declare const cond: boolean;
function removeNothing(y = cond ? true : undefined) {
    if (y !== undefined) {
        if (y === false) {
            return y;
        }
    }
    return true;
}
"#;
    let output = emit_dts(source);

    assert!(
        output.contains(
            "declare function removeUndefinedButNotFalse(x?: boolean): false | undefined;"
        ),
        "Expected false narrowing plus fallthrough undefined to be preserved: {output}"
    );
    assert!(
        output.contains("declare function removeNothing(y?: boolean | undefined): boolean;"),
        "Expected false/true branches to widen to boolean: {output}"
    );
}

#[test]
fn strip_internal_omits_exported_top_level_declarations() {
    let source = r#"
/** @internal */
export const stripped = 2;

/** @internal */
export function hiddenFunction() {}

/** @internal */
export interface HiddenInterface {
    value: string;
}

/** @internal */
export class HiddenClass {}

/** @internal */
export type HiddenAlias = string;

/** @internal */
export enum HiddenEnum {
    A,
}

/** @internal */
export namespace HiddenNamespace {
    export const value = 1;
}

export const visible = 3;
"#;
    let output = emit_dts_strip_internal(source);

    assert!(
        output.contains("visible"),
        "Expected visible export to remain: {output}"
    );
    for stripped_name in [
        "stripped",
        "hiddenFunction",
        "HiddenInterface",
        "HiddenClass",
        "HiddenAlias",
        "HiddenEnum",
        "HiddenNamespace",
        "@internal",
    ] {
        assert!(
            !output.contains(stripped_name),
            "Expected {stripped_name} to be stripped from declaration output: {output}"
        );
    }
}

#[test]
fn strip_internal_parameter_property_comments_do_not_emit_internal_fields() {
    let source = r#"
export class Foo {
  constructor(
    /** @internal */
    public isInternal1: string,
    /** @internal */ public isInternal2: string, /** @internal */
    public isInternal3: string,
    // @internal
    public isInternal4: string,
    // nothing
    /** @internal */
    public isInternal5: string,
    /* @internal */ public isInternal6: string,
    /* @internal */ public isInternal7: string, /** @internal */
    // not work
    public notInternal1: string,
    // @internal
    /* not work */
    public notInternal2: string,
    /* not work */
    // @internal
    /* not work */
    public notInternal3: string,
  ) { }
}

export class Bar {
  constructor(/* @internal */ public isInternal1: string) {}
}
"#;
    let output = emit_dts_strip_internal(source);

    for stripped_name in [
        "isInternal1",
        "isInternal2",
        "isInternal3",
        "isInternal4",
        "isInternal5",
        "isInternal6",
        "isInternal7",
    ] {
        assert!(
            !output.contains(&format!("    {stripped_name}: string;")),
            "Expected @internal parameter property field {stripped_name} to be stripped: {output}"
        );
    }
    assert!(
        output.contains("notInternal1: string;")
            && output.contains("notInternal2: string;")
            && output.contains("notInternal3: string;"),
        "Expected non-internal parameter properties to remain as class fields: {output}"
    );
    assert!(
        output.contains("constructor(\n    /** @internal */\n    isInternal1: string")
            && output.contains("/** @internal */ isInternal2: string")
            && output.contains("constructor(/* @internal */ isInternal1: string);"),
        "Expected constructor parameters to preserve relevant inline comments: {output}"
    );
}

#[test]
fn test_object_rest_with_keyword_property_names_omits_destructured_key() {
    let source = r#"
type P = {
    enum: boolean;
    function: boolean;
    abstract: boolean;
    async: boolean;
    await: boolean;
    one: boolean;
};

function f1({ enum: _enum, ...rest }: P) {
    return rest;
}

function f2({ function: _function, ...rest }: P) {
    return rest;
}

function f3({ abstract: _abstract, ...rest }: P) {
    return rest;
}

function f4({ async: _async, ...rest }: P) {
    return rest;
}

function f5({ await: _await, ...rest }: P) {
    return rest;
}
"#;
    let output = emit_dts_with_usage_analysis(source);

    for (function_name, omitted_key) in [
        ("f1", "enum"),
        ("f2", "function"),
        ("f3", "abstract"),
        ("f4", "async"),
        ("f5", "await"),
    ] {
        let signature = format!("declare function {function_name}");
        let start = output
            .find(&signature)
            .unwrap_or_else(|| panic!("Expected {signature} in declaration output: {output}"));
        let end = output[start..]
            .find("};")
            .map_or(output.len(), |offset| start + offset);
        let emitted_function = &output[start..end];

        assert!(
            !emitted_function.contains(&format!("    {omitted_key}: boolean;")),
            "Expected `{omitted_key}` to be omitted from {function_name} rest return type: {output}"
        );
    }

    assert!(
        output.contains("declare function f1({ enum: _enum, ...rest }: P):"),
        "Expected keyword binding pattern to be preserved in f1: {output}"
    );
    assert!(
        output.contains("declare function f5({ await: _await, ...rest }: P):"),
        "Expected keyword binding pattern to be preserved in f5: {output}"
    );
}

#[test]
fn test_non_exported_function_declaration_emits_declare_function() {
    let source = "function helper(x: string): string { return x; }";
    let (parser, root) = parse_test_source(source);

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
    let (parser, root) = parse_test_source(source);

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
    let (parser, root) = parse_test_source(source);

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
    let (parser, root) = parse_test_source(source);

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
    let (parser, root) = parse_test_source(source);

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
    let (parser, root) = parse_test_source(source);

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
fn test_asserted_class_property_initializer_retains_local_type_alias() {
    let output = emit_dts_with_usage_analysis(
        r#"
type N = 1;
export class Bar {
    c3? = 1 as N;
}
"#,
    );

    assert!(
        output.contains("type N = 1;"),
        "Expected asserted initializer alias to be retained: {output}"
    );
    assert!(
        output.contains("c3?: N;"),
        "Expected optional asserted property to use the alias without widening: {output}"
    );
    assert!(
        output.contains("export {};"),
        "Expected module marker when local alias is retained: {output}"
    );
}

#[test]
fn test_empty_named_export_has_no_extra_spacing() {
    let source = "export {};";
    let (parser, root) = parse_test_source(source);

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
fn test_js_local_enum_exports_are_deferred_before_alias_group() {
    let source = r#"
export enum A {}
enum B {}
export { B };
enum CC {}
export { CC as C };
export enum D {}
"#;
    let output = emit_js_dts_with_usage_analysis(source);

    let expected = r#"export enum A {
}
export enum D {
}
export enum B {
}
declare enum CC {
}
export { CC as C };
"#;

    assert_eq!(
        output, expected,
        "Expected local enum exports to match tsc order"
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
fn test_js_cjs_export_aliases_emit_at_first_alias_statement() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
exports.apply = undefined;
function a() {}
exports.apply = a;
"#,
    );

    let expected = "export { a as apply };\ndeclare function a(): void;\n";
    assert_eq!(
        output, expected,
        "Expected CJS alias group to keep the first alias statement position"
    );
}

#[test]
fn test_js_cjs_export_alias_with_later_values_emits_grouped_value_union() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
exports.apply = undefined;
exports.apply = undefined;
function a() {}
exports.apply = a;
exports.apply();
exports.apply = 'ok';
var OK = exports.apply.toUpperCase();
exports.apply = 1;
"#,
    );

    let expected = "export const apply: typeof a | \"ok\" | 1 | undefined;\nexport { a as apply };\ndeclare function a(): void;\n";
    assert_eq!(
        output, expected,
        "Expected CJS alias/value export to match tsc declaration grouping"
    );
}

#[test]
fn test_private_set_accessor_omits_type_and_uses_value_param_name() {
    let source = r#"
declare class C {
    private set x(foo: string);
}
"#;
    let (parser, root) = parse_test_source(source);

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
    let (parser, root) = parse_test_source(source);

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
fn test_returned_object_literal_member_comments_are_preserved() {
    let output = emit_dts(
        r#"
/**
 * make docs
 */
export const make = (value: string) => {
    return {
        /**
         * field docs
         */
        field: (next: number) => {},
        /**
         * method docs
         */
        method(next: number) {},
    };
}

export class Next {}
"#,
    );

    assert!(
        output.contains(
            "export declare const make: (value: string) => {\n    /** field docs */\n    field: (next: number) => void;\n    /** method docs */\n    method(next: number): void;\n};"
        ),
        "Expected returned object literal member JSDoc to stay with members: {output}"
    );
    assert!(
        !output.contains("*/\nexport declare class Next"),
        "Did not expect returned object member JSDoc to leak to the next declaration: {output}"
    );
}

#[test]
fn test_destructured_binding_comments_are_preserved_before_flattened_name() {
    let output = emit_dts(
        r#"
export let {
    /**
    * method docs
    */
    method
} = null as any;

declare global {
    interface Ext {
        method(): void;
    }
}
"#,
    );

    assert!(
        output.contains("export declare let \n/**\n* method docs\n*/\nmethod: any;"),
        "Expected destructured binding JSDoc to be emitted before the flattened name: {output}"
    );
    assert!(
        !output.contains("method: any;\n/**"),
        "Did not expect destructuring JSDoc to leak after the flattened declaration: {output}"
    );
}

#[test]
fn test_returned_local_uses_source_function_return_annotation_with_type_args() {
    let output = emit_dts_with_binding(
        r#"
export interface Box<T> {
    current: T;
}
export function box<T>(current: T): Box<T> {
    return { current };
}
export const useBox = () => {
    const value = box<typeof import("pkg")>(null);
    return value;
};
"#,
    );

    assert!(
        output.contains("export declare const useBox: () => Box<typeof import(\"pkg\")>;"),
        "Expected local call return annotation to preserve explicit type arguments: {output}"
    );
}

#[test]
fn test_generic_function_return_keeps_outer_type_param_over_later_alias() {
    let output = emit_dts_with_binding(
        r#"
function makeBox<T>(value: T) {
    return { value };
}

type Box<T> = ReturnType<typeof makeBox<T>>;
type StringBox = Box<string>;
"#,
    );

    assert!(
        output.contains("declare function makeBox<T>(value: T): {\n    value: T;\n};"),
        "Expected inferred object return to preserve the function type parameter: {output}"
    );
    assert!(
        !output.contains("declare function makeBox<T>(value: T): Box<string>;"),
        "Did not expect a later instantiated alias to replace the generic return: {output}"
    );
}

#[test]
fn test_instantiation_expression_error_recovery_matches_tsc_declarations() {
    let output = emit_dts_with_binding(
        r#"
declare let g: (<T>(x: T) => T) | undefined;
const c1 = g<string> || ((x: string) => x);
const c2 = g<string> ?? ((x: string) => x);
"#,
    );

    assert!(
        output.contains("declare const c1: (x: string) => string;"),
        "Expected || to collapse matching undefined fallback function: {output}"
    );
    assert!(
        output.contains("declare const c2: (x: string) => string;"),
        "Expected ?? to collapse matching undefined fallback function: {output}"
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
fn test_ts_class_getter_before_setter_preserves_source_order() {
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

    let getter_pos = output
        .find("get [G.B](): number;")
        .expect("missing getter in output");
    let setter_pos = output
        .find("set [G.B](value: number);")
        .expect("missing setter in output");

    assert!(
        getter_pos < setter_pos,
        "Expected TypeScript accessor declarations to preserve source order: {output}"
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

    // tsc emits late-bound computed methods as property-valued function types.
    assert!(
        output.contains("[key]: () => string;"),
        "Expected computed method to use property syntax (matching tsc): {output}"
    );
    assert!(
        !output.contains("[key](): string;"),
        "Did not expect method signature for late-bound computed method: {output}"
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
    let (parser, root) = parse_test_source(source);

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
        output.contains("@callback Foo") && output.contains("@type {Foo}"),
        "Expected callback and variable JSDoc comments to remain in declaration output: {output}"
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
fn test_js_variable_preserves_generic_jsdoc_type_reference() {
    let output = emit_js_dts(
        r#"
/** @template T @typedef {<T1 extends T>(data: T1) => T1} Test */

/** @type {Test<number>} */
const test = dibbity => dibbity
"#,
    );

    assert!(
        output.contains("declare const test: Test<number>;"),
        "Expected generic JSDoc @type alias reference to be preserved: {output}"
    );
    assert!(
        output.contains("type Test<T> = <T1 extends T>(data: T1) => T1;"),
        "Expected same-line @template/@typedef alias to emit clean type parameters: {output}"
    );
}

#[test]
fn test_js_variable_normalizes_legacy_dot_generic_jsdoc_type_reference() {
    let output = emit_js_dts(
        r#"
/** @type {Array.<number>} */
const values = [];
"#,
    );

    assert!(
        output.contains("declare const values: Array<number>;"),
        "Expected legacy JSDoc dot-generic form to normalize to standard generic syntax: {output}"
    );
    assert!(
        !output.contains(": Array.<number>;"),
        "Did not expect invalid legacy dot-generic syntax in emitted type annotation: {output}"
    );
}

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
        output.contains(
            "/**\n * @param {ResolveRejectMap} handlers\n * @returns {Promise<any>}\n */\ndeclare function send"
        ),
        "Expected function variable JSDoc to stay attached to synthetic declaration: {output}"
    );
    assert!(
        output.contains("type ResolveRejectMap = {\n    [id: string]: [Function, Function];\n};"),
        "Expected multiline JSDoc typedef alias to be emitted as a local type alias: {output}"
    );
}

#[test]
fn test_js_multiline_typedef_before_variable_comment_is_preserved() {
    let source = r#"
/**
 * @typedef {{
 *   [id: string]: [Function, Function];
 * }} ResolveRejectMap
 */
let id = 0;

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
        output.starts_with(
            "/**\n * @typedef {{\n * [id: string]: [Function, Function];\n * }} ResolveRejectMap\n */\ndeclare let id: number;"
        ),
        "Expected source typedef comment to stay attached to the variable declaration: {output}"
    );
    assert!(
        output.contains("declare function send(handlers: ResolveRejectMap): Promise<any>;"),
        "Expected function variable to use the typedef alias: {output}"
    );
    assert!(
        output.contains("type ResolveRejectMap = {\n    [id: string]: [Function, Function];\n};"),
        "Expected typedef alias to still be emitted: {output}"
    );
}

#[test]
fn test_js_multiline_typedef_preserves_unstarred_source_lines() {
    let output = emit_js_dts(
        r#"
/**
 * @template T
 * @typedef {{
  value: {
    [K in keyof T]?: Box<T[K]>[]
  }
}} Box<T> */
/** @type {Box<{foo:string}>} */
const p = {};
"#,
    );

    assert!(
        output.starts_with(
            "/**\n * @template T\n * @typedef {{\n  value: {\n    [K in keyof T]?: Box<T[K]>[]\n  }\n}} Box<T> */\n/** @type {Box<{foo:string}>} */\ndeclare const p: Box<{"
        ),
        "Expected unstarred typedef lines and following @type comment to preserve source text: {output}"
    );
    assert!(
        output.contains("type Box<T> = {\n    value: { [K in keyof T]?: Box<T[K]>[]; };\n};"),
        "Expected generic typedef name suffix to be folded into type parameters: {output}"
    );
}

#[test]
fn test_js_multiline_typedef_before_export_equals_function_variable_is_emitted() {
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
module.exports = send;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    let export_pos = output
        .find("export = send;")
        .expect("Expected CommonJS export-equals statement");
    let function_pos = output
        .find("declare function send(handlers: ResolveRejectMap): Promise<any>;")
        .expect("Expected synthetic function declaration for send");
    assert!(
        export_pos < function_pos,
        "Expected export= send to emit before the synthetic declaration in CommonJS mode: {output}"
    );
    assert!(
        output.contains("type ResolveRejectMap = {\n    [id: string]: [Function, Function];\n};"),
        "Expected multiline JSDoc typedef alias to be emitted alongside export= send: {output}"
    );
    assert_eq!(
        output.matches("export = send;").count(),
        1,
        "Did not expect duplicate export= send statements: {output}"
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
fn test_js_function_declaration_emits_separate_jsdoc_overload_comments() {
    let output = emit_js_dts(
        r#"
/**
 * @overload
 * @param {number} value
 * @returns {'number'}
 */
/**
 * @overload
 * @param {string} value
 * @returns {'string'}
 */
/**
 * @param {unknown} value
 * @returns {string}
 */
function kind(value) {
  return typeof value;
}

/**
 * @template T
 * @param {T} value
 * @returns {T}
 */
const identity = value => value;

/**
 * @template T
 * @overload
 * @param {T[]} values
 * @returns {T[]}
 */
/**
 * @param {unknown[]} values
 * @returns {unknown[]}
 */
function copy(values) {
  return values.map(identity);
}
"#,
    );

    let kind_number = output
        .find("declare function kind(value: number): \"number\";")
        .expect("expected number overload");
    let kind_string = output
        .find("declare function kind(value: string): \"string\";")
        .expect("expected string overload");
    let copy = output
        .find("declare function copy<T>(values: T[]): T[];")
        .expect("expected generic overload");
    let identity = output
        .find("declare function identity<T>(value: T): T;")
        .expect("expected variable function declaration");

    assert!(
        kind_number < kind_string && kind_string < copy && copy < identity,
        "Expected JS function overloads to stay in function source order before function variables: {output}"
    );
    assert!(
        !output.contains("declare function kind(value: unknown): string;"),
        "Implementation signature should not be emitted for @overload JSDoc: {output}"
    );
}

#[test]
fn test_js_function_declaration_emits_combined_jsdoc_overload_comment() {
    let output = emit_js_dts(
        r#"
/**
 * @template T
 * @template U
 * @overload
 * @param {T[]} array
 * @param {(x: T) => U[]} mapper
 * @returns {U[]}
 *
 * @overload
 * @param {T[][]} array
 * @returns {T[]}
 *
 * @param {unknown[]} array
 * @param {(x: unknown) => unknown} mapper
 * @returns {unknown[]}
 */
function flatMap(array, mapper) {
  return [];
}
"#,
    );

    assert!(
        output.contains("declare function flatMap<T, U>(array: T[], mapper: (x: T) => U[]): U[];"),
        "Expected first overload from combined JSDoc comment: {output}"
    );
    assert!(
        output.contains("declare function flatMap<T, U>(array: T[][]): T[];"),
        "Expected second overload from combined JSDoc comment: {output}"
    );
    assert!(
        !output.contains("array: unknown[]"),
        "Implementation JSDoc tags after overloads should not become a declaration signature: {output}"
    );
}

#[test]
fn test_js_method_declaration_emits_jsdoc_overload_comments() {
    let output = emit_js_dts(
        r#"
/**
 * @template T
 */
class Box {
  /** @param {T} value */
  constructor(value) {
    this.value = value;
  }

  /**
   * @overload
   * @param {Box<number>} this
   * @returns {'number'}
   */
  /**
   * @overload
   * @param {Box<string>} this
   * @returns {'string'}
   */
  /**
   * @returns {string}
   */
  kind() {
    return typeof this.value;
  }
}
"#,
    );

    assert!(
        output.contains("kind(this: Box<number>): \"number\";"),
        "Expected number receiver overload: {output}"
    );
    assert!(
        output.contains("kind(this: Box<string>): \"string\";"),
        "Expected string receiver overload: {output}"
    );
    assert!(
        !output.contains("kind(): string;"),
        "Implementation method signature should not be emitted for JSDoc overloads: {output}"
    );
}

#[test]
fn test_js_constructor_declaration_emits_jsdoc_overloads_before_private_marker() {
    let output = emit_js_dts(
        r#"
export class Foo {
  #value;

  /**
   * @constructor
   * @overload
   * @param {string} value
   */
  /**
   * @constructor
   * @overload
   * @param {number} value
   */
  /** @constructor @param {string | number} value */
  constructor(value) {
    this.#value = value;
  }
}
"#,
    );

    let string_ctor = output
        .find("constructor(value: string);")
        .expect("expected string constructor overload");
    let number_ctor = output
        .find("constructor(value: number);")
        .expect("expected number constructor overload");
    let private_marker = output.find("#private;").expect("expected private marker");

    assert!(
        string_ctor < number_ctor && number_ctor < private_marker,
        "Expected constructor overloads before private marker: {output}"
    );
    assert!(
        !output.contains("string | number"),
        "Implementation constructor JSDoc should not become a signature: {output}"
    );
}

#[test]
fn test_js_object_namespace_emits_legacy_jsdoc_overload_member_comments() {
    let output = emit_js_dts(
        r#"
const example = {
  /**
   * @overload Example(value)
   *   Creates Example
   *   @param value [String]
   */
  constructor: function Example(value, options) {},
};
"#,
    );

    assert!(
        output.contains("declare namespace example"),
        "Expected object literal namespace declaration: {output}"
    );
    assert!(
        output.contains("@overload Example(value)"),
        "Expected legacy overload comment to be preserved: {output}"
    );
    assert!(
        output.contains("function constructor(value: any): any;"),
        "Expected legacy overload params to replace the implementation signature: {output}"
    );
    assert!(
        !output.contains("options:"),
        "Implementation-only parameters should not leak into the legacy overload: {output}"
    );
}

#[test]
fn test_js_object_namespace_aliases_multiple_legacy_constructor_overloads() {
    let output = emit_js_dts(
        r#"
const example = {
  /**
   * @overload Example(value)
   * @param value [String]
   * @param secret [String]
   * @overload Example(options)
   * @option options value [String]
   */
  constructor: function Example() {},
};
"#,
    );

    assert!(
        output.contains("export function constructor_1(value: any, secret: any): any;"),
        "Expected first legacy constructor overload to use an aliasable local name: {output}"
    );
    assert!(
        output.contains("export function constructor_1(): any;"),
        "Expected option-only legacy overload to fall back to no parameters: {output}"
    );
    assert!(
        output.contains("export { constructor_1 as constructor };"),
        "Expected constructor alias export after synthetic overloads: {output}"
    );
}

#[test]
fn test_js_object_namespace_malformed_legacy_overload_falls_back_to_no_params() {
    let output = emit_js_dts(
        r#"
const example = {
  /**
   * @overload evaluate(options = {}, [callback])
   * @param options [map]
   * @callback callback function (error, result)
   *   If callback is provided it will be called with evaluation result
   *   @param error [Error]
   *   @param result [String]
   */
  evaluate: function evaluate(options, callback) {},
};
"#,
    );

    assert!(
        output.contains("function evaluate(): any;"),
        "Expected malformed legacy overload call to fall back to a no-arg any signature: {output}"
    );
    assert!(
        !output.contains("options:"),
        "Malformed legacy overload params should not be trusted as a signature: {output}"
    );
    assert!(
        output.contains("type callback = (error: any, result: any) => any;"),
        "Expected nested legacy @callback alias to be emitted after the namespace: {output}"
    );
}

#[test]
fn test_js_function_variable_strips_jsdoc_satisfies_comment() {
    let output = emit_js_dts(
        r#"
/** @satisfies {(uuid: string) => void} */
export const fn1 = uuid => {};

/**
 * @satisfies {(a: string, ...args: never) => void}
 * @param {string} a
 */
export const fn2 = (a, b) => {};

/** @satisfies {(uuid: string) => void} */
export function fn3(uuid) {}
"#,
    );

    assert!(
        !output.contains("@satisfies {(uuid: string) => void} */\nexport function fn1"),
        "Expected synthetic function-variable JSDoc @satisfies comment to be stripped: {output}"
    );
    assert!(
        !output.contains("@satisfies {(a: string, ...args: never) => void}"),
        "Expected multiline synthetic function-variable @satisfies comment to be stripped: {output}"
    );
    assert!(
        output.contains("export function fn1(uuid: string): void;"),
        "Expected @satisfies parameter fallback to remain active: {output}"
    );
    assert!(
        output.contains("export function fn2(a: string, b: never): void;"),
        "Expected @param plus @satisfies inference to remain active: {output}"
    );
    assert!(
        output.contains(
            "/** @satisfies {(uuid: string) => void} */\nexport function fn3(uuid: any): void;"
        ),
        "Expected function declarations to preserve @satisfies comments: {output}"
    );
}

#[test]
fn test_js_function_declaration_emits_constrained_jsdoc_template() {
    let output = emit_js_dts(
        r#"
/**
 * @template {string} T
 * @param {T} x
 * @returns {T}
 */
export function id(x) {
  return x;
}
"#,
    );

    assert!(
        output.contains("export function id<T extends string>(x: T): T;"),
        "Expected constrained JSDoc template to emit as a type parameter constraint: {output}"
    );
    assert!(
        !output.contains("id<{string}, T>"),
        "Did not expect braced JSDoc constraint to emit as a fake type parameter: {output}"
    );
}

#[test]
fn test_js_function_declaration_uses_jsdoc_type_alias_signature() {
    let output = emit_js_dts(
        r#"
/**
 * @typedef {<T>(m : T) => T} IFn
 */

/** @type {IFn} */
export function inJs(l) {
  return l;
}
"#,
    );

    assert!(
        output.contains("export function inJs<T>(m: T): T;"),
        "Expected JSDoc @type function alias to emit as a function signature: {output}"
    );
    assert!(
        output.contains("export type IFn = <T>(m: T) => T;"),
        "Expected the JSDoc typedef alias to still be emitted: {output}"
    );
    assert!(
        !output.contains("@type {IFn}"),
        "Did not expect implementation-only @type comment in declaration output: {output}"
    );
}

#[test]
fn test_js_function_declaration_uses_jsdoc_type_alias_signature_with_nested_commas() {
    let output = emit_js_dts(
        r#"
/**
 * @typedef {<T>(x: [T, number], y: { items: [T, string] }) => [T, string]} IFn
 */

/** @type {IFn} */
export function inJs(l) {
  return l;
}
"#,
    );

    assert!(
        output.contains(
            "export function inJs<T>(x: [T, number], y: { items: [T, string] }): [T, string];"
        ),
        "Expected nested tuple/object commas in JSDoc function typedef to parse as a single signature: {output}"
    );
    assert!(
        output.contains("export type IFn = <T>(x: [T, number], y: {")
            && output.contains("items: [T, string];")
            && output.contains("}) => [T, string];"),
        "Expected nested tuple/object commas to be preserved in emitted typedef alias structure: {output}"
    );
}

#[test]
fn test_js_function_declaration_uses_jsdoc_type_alias_signature_with_nested_function_param() {
    let output = emit_js_dts(
        r#"
/**
 * @typedef {(cb: (x: number) => string, value: number) => void} IFn2
 */

/** @type {IFn2} */
export function inJs(cb, value) {
  cb(value);
}
"#,
    );

    assert!(
        output.contains("export function inJs(cb: (x: number) => string, value: number): void;"),
        "Expected nested function parameter type to parse through closing paren matching: {output}"
    );
    assert!(
        output.contains("export type IFn2 = (cb: (x: number) => string, value: number) => void;"),
        "Expected emitted typedef alias to preserve nested function parameter type: {output}"
    );
}

#[test]
fn test_js_function_declaration_type_alias_signature_preserves_non_type_jsdoc_comments() {
    let output = emit_js_dts(
        r#"
/**
 * @typedef {<T>(m : T) => T} IFn
 */

/**
 * Keep this function-level JSDoc.
 * @deprecated use next
 */
/** @type {IFn} */
export function inJs(l) {
  return l;
}
"#,
    );

    assert!(
        output.contains("export function inJs<T>(m: T): T;"),
        "Expected JSDoc @type function alias to emit as a function signature: {output}"
    );
    assert!(
        output.contains("@deprecated use next"),
        "Expected non-@type JSDoc comments to remain in declaration output: {output}"
    );
    assert!(
        !output.contains("@type {IFn}"),
        "Did not expect implementation-only @type comment in declaration output: {output}"
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
fn test_js_named_export_function_preserves_jsdoc_signature_at_export_position() {
    let output = emit_js_dts(
        r#"
export function b() {}

/**
 * @param {{x: string}} a
 * @param {{y: typeof b}} b
 */
function g(a, b) {
    return a.x && b.y();
}

export { g };
"#,
    );

    assert!(
        output.contains("export function g(a: {\n    x: string;\n}, b: {\n    y: typeof import(\".\").b;\n}): void | \"\";"),
        "Expected folded JS export function to preserve JSDoc param and return types: {output}"
    );
    assert!(
        output.contains(
            "/**\n * @param {{x: string}} a\n * @param {{y: typeof b}} b\n */\nexport function g"
        ),
        "Expected folded JS export function to keep its JSDoc comment: {output}"
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
fn test_js_namespace_named_export_keeps_required_constructor_import_type() {
    let source = r#"
export const Something = 2;
export namespace A {
    export namespace B {
        const Something = require("fs").Something;
        const thing = new Something();
        export { thing };
    }
}
"#;
    let output = emit_js_dts_with_usage_analysis(source);

    assert!(
        output.contains("export namespace A {\n    namespace B {\n        export { thing };\n        export let thing: import(\"fs\").Something;\n    }\n}"),
        "Expected namespace named export to emit a reusable import type after its export clause: {output}"
    );
}

#[test]
fn test_js_module_exports_object_uses_require_property_import_alias() {
    let source = r#"
const Something = require("fs").Something;
const thing = new Something();
module.exports = {
    thing
};
"#;
    let output = emit_js_dts_with_usage_analysis(source);

    assert_eq!(
        output.trim(),
        "export const thing: Something;\nimport Something_1 = require(\"fs\");\nimport Something = Something_1.Something;"
    );
}

#[test]
fn test_js_nested_module_exports_object_emits_namespace_with_import_alias() {
    let source = r#"
const Something = require("fs").Something;
module.exports.A = {}
module.exports.A.B = {
    thing: new Something()
}
"#;
    let output = emit_js_dts_with_usage_analysis(source);

    assert_eq!(
        output.trim(),
        "export namespace A {\n    namespace B {\n        let thing: Something;\n    }\n}\nimport Something_1 = require(\"fs\");\nimport Something = Something_1.Something;"
    );
}

#[test]
fn test_js_exported_object_literal_namespace_records_new_expression_import_alias() {
    let source = r#"
const Something = require("fs").Something;
const ns = {
    thing: new Something()
};
export { ns };
"#;
    let output = emit_js_dts_with_usage_analysis(source);

    assert!(
        output.contains("export namespace ns {\n    let thing: Something;\n}"),
        "Expected exported object literal to emit as a namespace with a constructor member: {output}"
    );
    assert!(
        output.contains(
            "import Something_1 = require(\"fs\");\nimport Something = Something_1.Something;"
        ),
        "Expected new-expression constructor type to record its require-property import alias: {output}"
    );
}

#[test]
fn test_js_require_property_import_alias_avoids_existing_module_alias_name() {
    let source = r#"
const Something = require("fs").Something;
const Something_1 = 1;
const thing = new Something();
module.exports = {
    thing,
    Something_1
};
"#;
    let output = emit_js_dts_with_usage_analysis(source);

    assert!(
        output.contains("export const Something_1: 1;"),
        "Expected real exported binding to keep its name: {output}"
    );
    assert!(
        output.contains(
            "import Something_2 = require(\"fs\");\nimport Something = Something_2.Something;"
        ),
        "Require-property module alias should skip the real Something_1 binding: {output}"
    );
    assert!(
        !output.contains("import Something_1 = require(\"fs\");"),
        "Synthetic module alias must not collide with the real Something_1 binding: {output}"
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
    let (parser, root) = parse_test_source(source);

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
fn test_js_module_exports_function_with_typedef_members() {
    let output = emit_js_dts(
        r#"
/**
 * @typedef Options
 * @property {string} opt
 */

/**
 * @param {Options} options
 */
module.exports = function loader(options) {}
"#,
    );

    let expected = r#"declare namespace _exports {
    export { Options };
}
declare function _exports(options: Options): void;
export = _exports;
type Options = {
    opt: string;
};"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected export= function to retain local typedef namespace members: {output}"
    );
}

#[test]
fn test_export_equals_namespace_keeps_local_type_dependencies() {
    let source = r#"
namespace X {
    interface A {
        kind: 'a';
    }

    interface B {
        kind: 'b';
    }

    export type C = A | B;
}

export = X;
"#;

    let output = emit_dts_with_usage_analysis(source);

    assert!(
        output.contains("interface A {\n        kind: 'a';\n    }"),
        "Expected local namespace interface A used by exported alias to be retained: {output}"
    );
    assert!(
        output.contains("interface B {\n        kind: 'b';\n    }"),
        "Expected local namespace interface B used by exported alias to be retained: {output}"
    );
    assert!(
        output.contains("export type C = A | B;"),
        "Expected exported namespace alias to be retained: {output}"
    );
    assert!(
        output.contains("export {};"),
        "Expected mixed exported and local namespace members to emit a scope marker: {output}"
    );
}

#[test]
fn test_namespace_shadowed_default_export_uses_self_import_type_names() {
    let (parser, _root) = parse_test_source("");
    let mut emitter = DeclarationEmitter::new(&parser.arena);
    emitter.current_namespace_self_import_alias = Some("me".to_string());
    emitter.current_namespace_shadowed_default_name = Some("MyComponent".to_string());
    emitter.current_namespace_self_export_names.extend([
        "Things".to_string(),
        "Props".to_string(),
        "MyComponent".to_string(),
    ]);

    let qualified = emitter.qualify_current_namespace_self_type_text("Things<Props, MyComponent>");

    assert_eq!(qualified, "me.Things<me.Props, me.default>");
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
fn test_js_commonjs_keyword_named_exports_emit_aliases() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
exports.class = 123;
exports.for = "loop";
"#,
    );

    assert!(
        output.contains("declare const _class: 123;"),
        "Expected reserved export name to use a local alias: {output}"
    );
    assert!(
        output.contains("declare const _for: \"loop\";"),
        "Expected reserved export name to use a local alias: {output}"
    );
    assert!(
        output.contains("export { _class as class, _for as for };"),
        "Expected reserved export aliases to be grouped: {output}"
    );
    assert!(
        !output.contains("export const class"),
        "Did not expect invalid keyword binding declaration: {output}"
    );
    assert!(
        !output.contains("export const for"),
        "Did not expect invalid keyword binding declaration: {output}"
    );
}

#[test]
fn test_js_module_exports_object_keyword_name_and_namespace_members() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
var x = 12;
module.exports = {
    extends: "base",
    more: {
        others: ["strs"]
    },
    x
};
"#,
    );

    assert!(
        output.contains("export var x: number;"),
        "Expected shorthand object export to keep JS var widening: {output}"
    );
    assert!(
        output.contains("declare let _extends: string;"),
        "Expected reserved object export name to use a local alias: {output}"
    );
    assert!(
        output.contains("export declare namespace more {\n    let others: string[];\n}"),
        "Expected nested object member to emit as an export namespace: {output}"
    );
    assert!(
        output.contains("export { _extends as extends };"),
        "Expected reserved object export alias to be grouped: {output}"
    );
    assert!(
        !output.contains("export const x: 12;"),
        "Did not expect the JS var export to remain const-narrowed: {output}"
    );
    assert!(
        !output.contains("export const extends"),
        "Did not expect invalid keyword binding declaration: {output}"
    );
}

#[test]
fn test_js_commonjs_bracket_string_exports_emit_named_declarations() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
exports["foo"] = 1;
module.exports["bar"] = "x";
"#,
    );

    assert!(
        output.contains("export const foo: 1;"),
        "Expected bracket string exports to emit named declarations: {output}"
    );
    assert!(
        output.contains("export const bar: \"x\";"),
        "Expected module.exports bracket string exports to emit named declarations: {output}"
    );
}

#[test]
fn test_js_commonjs_element_access_invalid_export_alias() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
function D() {}
exports["D"] = D;
/** alias comment should stay attached to the skipped source statement */
exports["Does not work yet"] = D;
"#,
    );

    assert!(
        output.contains("export function D(): void;"),
        "Expected valid element access export to emit the local function: {output}"
    );
    assert!(
        output.contains("export { D as _Does_not_work_yet };"),
        "Expected invalid element access export name to emit a sanitized alias: {output}"
    );
    assert!(
        !output.contains("alias comment should stay attached"),
        "Did not expect skipped alias statement comments to leak into output: {output}"
    );
}

#[test]
fn test_jsdoc_object_param_properties_type_destructured_parameter() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @param {object} opts
 * @param {number} opts.a
 * @param {number} [opts.b]
 * @returns {number}
 */
function foo({ a, b }) {
    return a + (b ?? 0);
}
"#,
    );

    assert!(
        output.contains(
            "declare function foo({ a, b }: {\n    a: number;\n    b?: number | undefined;\n}): number;"
        ),
        "Expected JSDoc object property tags to type the destructured parameter: {output}"
    );
}

#[test]
fn test_jsdoc_nested_object_param_properties_type_destructured_parameter() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @param {Object} opts
 * @param {string?} opts.reason
 * @param {Object} opts.suberr
 * @param {string?} opts.suberr.reason
 * @param {string?} opts.suberr.code
 */
function foo({ reason, suberr }) {}
"#,
    );

    assert!(
        output.contains(
            "declare function foo({ reason, suberr }: {\n    reason: string | null;\n    suberr: {\n        reason: string | null;\n        code: string | null;\n    };\n}): void;"
        ),
        "Expected nested JSDoc object property tags to type the destructured parameter: {output}"
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
fn test_js_commonjs_define_property_exports_emit_named_declarations() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
exports.named = 1;
Object.defineProperty(exports, "myProp", { value: 42, writable: true });
Object.defineProperty(module.exports, "ro", { value: "fixed" });
"#,
    );

    assert!(
        output.contains("export const named: 1;"),
        "Expected assignment-shaped CommonJS export declaration: {output}"
    );
    assert!(
        output.contains("export const myProp: number;"),
        "Expected Object.defineProperty(exports, ...) declaration: {output}"
    );
    assert!(
        output.contains("export const ro: string;"),
        "Expected Object.defineProperty(module.exports, ...) declaration: {output}"
    );
}

#[test]
fn test_js_commonjs_define_property_only_export_marks_public_api() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
Object.defineProperty(exports, "only", { value: 42 });
var local = 123;
"#,
    );

    assert!(
        output.contains("export const only: number;"),
        "Expected defineProperty-only CommonJS export declaration: {output}"
    );
    assert!(
        !output.contains("declare var local:"),
        "Did not expect local declarations to leak from a defineProperty-only module: {output}"
    );
}

#[test]
fn test_js_commonjs_define_property_function_exports() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
function fn() {}

Object.defineProperty(module.exports, "fn", { value: fn });
Object.defineProperty(module.exports, "alias", { value: module.exports.fn });
Object.defineProperty(module.exports.fn, "self", { value: module.exports.fn });
"#,
    );

    assert!(
        output.contains("export function fn(): void;"),
        "Expected local function defineProperty export: {output}"
    );
    assert!(
        output.contains("export function alias(): void;"),
        "Expected defineProperty export alias to reuse the function signature: {output}"
    );
    assert!(
        output.contains("export namespace fn {\n    function self(): void;\n}"),
        "Expected defineProperty namespace member function declaration: {output}"
    );
    assert!(
        !output.contains("declare function fn"),
        "Did not expect the consumed local function to be emitted separately: {output}"
    );
}

#[test]
fn test_js_esm_syntax_ignores_commonjs_named_exports() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
export const x = 0;
module.exports.y = 0;
"#,
    );

    assert!(
        output.contains("export const x: 0;"),
        "Expected native ESM export to remain: {output}"
    );
    assert!(
        !output.contains("export const y:"),
        "Did not expect CommonJS assignment to become a named export in an ESM JS file: {output}"
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
fn test_js_commonjs_object_export_function_infers_binary_return_from_jsdoc_param() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @param {string} a
 */
function bar(a) {
    return a + a;
}

module.exports = { bar };
"#,
    );

    assert!(
        output.contains("export function bar(a: string): string;"),
        "Expected JSDoc parameter type to infer the CommonJS function return: {output}"
    );
}

#[test]
fn test_js_commonjs_object_export_preserves_documented_source_order() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * const doc comment
 */
const x = (a) => {
    return "";
};

/**
 * function doc comment
 */
function b() {
    return 0;
}

module.exports = { x, b };
"#,
    );

    let x_pos = output
        .find("/**\n * const doc comment\n */\nexport function x(a: any): string;")
        .unwrap_or_else(|| panic!("Expected documented exported function x: {output}"));
    let b_pos = output
        .find("/**\n * function doc comment\n */\nexport function b(): number;")
        .unwrap_or_else(|| panic!("Expected documented exported function b: {output}"));
    assert!(
        x_pos < b_pos,
        "Expected module.exports object declarations to preserve source order: {output}"
    );
}

#[test]
fn test_jsdoc_enum_object_literal_emits_type_and_namespace() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/** @enum {string} */
export const Target = {
    START: "start",
    /** @type {number} */
    OK_I_GUESS: 2
};

/** @enum {function(number): number} */
export const Fs = {
    ADD1: n => n + 1,
    SUB1: n => n - 1
};

/** @enum {?} */
export const Unknowns = { ANY: 1 };

/** @enum {Array} */
export const Lists = { EMPTY: [] };

/** @enum {Promise} */
export const Tasks = { DONE: Promise.resolve() };

/** @enum {function(Array): Promise} */
export const AsyncFns = { RUN: values => Promise.resolve(values) };
"#,
    );

    assert!(
        output.contains("export type Target = string;\nexport namespace Target {"),
        "Expected JSDoc enum value to emit type plus namespace: {output}"
    );
    assert!(
        output.contains("let START: string;"),
        "Expected enum members to use the enum base type: {output}"
    );
    assert!(
        output.contains("let OK_I_GUESS: number;"),
        "Expected member @type to override the enum base type: {output}"
    );
    assert!(
        output.contains("export type Fs = (arg0: number) => number;"),
        "Expected function enum base type to normalize to arrow function syntax: {output}"
    );
    assert!(
        output.contains("function ADD1(n: any): any;")
            && output.contains("function SUB1(n: any): any;"),
        "Expected function enum members to emit as namespace functions: {output}"
    );
    assert!(
        output.contains("export type Unknowns = any;"),
        "Expected standalone Closure unknown enum type to normalize to any: {output}"
    );
    assert!(
        output.contains("export type Lists = any[];"),
        "Expected bare Array enum type to normalize to any[]: {output}"
    );
    assert!(
        output.contains("export type Tasks = Promise<any>;"),
        "Expected bare Promise enum type to normalize to Promise<any>: {output}"
    );
    assert!(
        output.contains("export type AsyncFns = (arg0: any[]) => Promise<any>;"),
        "Expected enum function type to use JSDoc function normalization: {output}"
    );
}

#[test]
fn test_jsdoc_missing_generic_arguments_default_to_any() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @param {Array=} values
 */
function takesArray(values) {}

/** @param {Promise} promise */
function takesPromise(promise) {}

/** @param {function(Array)} callback */
function takesCallback(callback) {}

/**
 * @return {?Promise}
 */
function maybePromise() {
    return null;
}
"#,
    );

    assert!(
        output.contains("declare function takesArray(values?: any[] | undefined): void;"),
        "Expected optional bare Array to become any[] | undefined: {output}"
    );
    assert!(
        output.contains("declare function takesPromise(promise: Promise<any>): void;"),
        "Expected bare Promise to become Promise<any>: {output}"
    );
    assert!(
        output.contains("declare function takesCallback(callback: (arg0: any[]) => any): void;"),
        "Expected function(Array) to use any[] and default any return: {output}"
    );
    assert!(
        output.contains("declare function maybePromise(): Promise<any> | null;"),
        "Expected nullable bare Promise return to become Promise<any> | null: {output}"
    );
}

#[test]
fn test_js_commonjs_default_function_export_is_renamed_to_default_alias() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
exports.default = function (x) {
    return x;
};
"#,
    );

    assert!(
        output.contains("declare function _default(x: any);"),
        "Expected CJS default function export to use a synthetic default alias: {output}"
    );
    assert!(
        output.contains("export default _default;"),
        "Expected CJS default export to emit a default alias line: {output}"
    );
    assert!(
        !output.contains("export function default"),
        "Expected reserved default export name to be rewritten: {output}"
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
fn test_js_function_class_merge_omits_non_constructor_signature() {
    let output = emit_js_dts(
        r#"
function C1() {
    /**
     * @param {number} x
     * @param {number} y
     * @returns {number}
     */
    this.prop = function (x, y) {
        return x + y;
    };
}

/**
 * @param {number} x
 * @param {number} y
 * @returns {number}
 */
C1.prototype.method = function (x, y) {
    return x + y;
};
"#,
    );

    assert!(
        output.contains("declare function C1(): void;\ndeclare class C1 {"),
        "Expected JS function plus prototype members to merge with a companion class: {output}"
    );
    assert!(
        !output.contains("constructor();"),
        "Expected companion class not to duplicate the already-emitted function signature as a constructor: {output}"
    );
    assert!(
        output.contains("prop: (x: number, y: number) => number;")
            && output.contains("method(x: number, y: number): number;"),
        "Expected constructor-assigned and prototype members to remain in the companion class: {output}"
    );
}

#[test]
fn test_js_class_static_expando_namespace_members_are_ambient_members() {
    let output = emit_js_dts(
        r#"
function C1() {}

/**
 * @param {number} x
 * @param {number} y
 * @returns {number}
 */
C1.staticProp = function (x, y) {
    return x + y;
};

class C2 {}

/**
 * @param {number} x
 * @param {number} y
 * @returns {number}
 */
C2.staticProp = function (x, y) {
    return x + y;
};
"#,
    );

    assert!(
        output.contains("declare namespace C1 {\n    /**")
            && output.contains("    function staticProp(x: number, y: number): number;\n}"),
        "Expected function static expando to emit as an ambient namespace member: {output}"
    );
    assert!(
        output.contains("declare namespace C2 {\n    /**")
            && output.contains("    function staticProp(x: number, y: number): number;\n}"),
        "Expected class static expando to emit as an ambient namespace member: {output}"
    );
    assert!(
        !output.contains("export function staticProp"),
        "Did not expect explicit export on ambient namespace members: {output}"
    );
}

#[test]
fn test_var_array_initializer_with_index_assignment_emits_valid_array_type() {
    // Regression: `var t = [1, 2, 3]; t[0] = 5;` previously emitted
    // `declare var t: {\n    : number[];` — invalid TypeScript, because
    // the late-bound expando function path wrote the opening `: {` before
    // checking whether the initializer was a function. For an array
    // literal the function then bailed out, leaving a partial brace in
    // the output. After the fix the initializer shape is probed first
    // and the late-bound path is skipped entirely.
    let output = emit_dts(
        r#"
var t = [1, 2, 3];
t[0] = 5;
"#,
    );
    assert!(
        output.contains("declare var t: number[];"),
        "Expected valid array type for var with index assignment, got: {output}"
    );
    assert!(
        !output.contains(": {\n    : "),
        "Did not expect partial broken object type in output: {output}"
    );
    assert!(
        !output.contains("Array<>"),
        "Did not expect raw Array<> token: {output}"
    );
}

#[test]
fn test_var_array_initializer_with_property_assignment_emits_valid_array_type() {
    // Same regression as above but for property-style assignment
    // (`t.foo = 5`). Both element-access and property-access assignments
    // were triggering `collect_ts_late_bound_assignment_members`, which
    // in turn entered the broken `: {` write path.
    let output = emit_dts(
        r#"
var t = [1, 2, 3];
t.foo = 5;
"#,
    );
    assert!(
        output.contains("declare var t: number[];"),
        "Expected valid array type for var with property assignment, got: {output}"
    );
    assert!(
        !output.contains(": {\n    : "),
        "Did not expect partial broken object type in output: {output}"
    );
}

#[test]
fn test_const_array_initializer_with_index_assignment_emits_valid_array_type() {
    // Same as above for `const` declarations.
    let output = emit_dts(
        r#"
const t = [1, 2, 3];
t[0] = 5;
"#,
    );
    assert!(
        output.contains("declare const t"),
        "Expected valid array type for const with index assignment, got: {output}"
    );
    assert!(
        !output.contains(": {\n    : "),
        "Did not expect partial broken object type in output: {output}"
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

    let (parser, root) = parse_test_source(source);
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
fn test_mutable_generic_call_literal_result_widens_in_declaration_emit() {
    let source = r#"
function foo<T>(x: T) { return x; }
var x = foo(5);
"#;
    let (parser, root) = parse_test_source(source);
    let root_node = parser.arena.get(root).expect("missing root node");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file");
    let get_var_decl = |stmt_idx: NodeIndex| {
        parser
            .arena
            .get(stmt_idx)
            .and_then(|node| parser.arena.get_variable(node))
            .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
            .and_then(|node| parser.arena.get_variable(node))
            .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
            .and_then(|node| parser.arena.get_variable_declaration(node))
            .expect("missing variable declaration")
    };
    let var_x_decl = get_var_decl(source_file.statements.nodes[1]);

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);
    let interner = TypeInterner::new();
    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    let literal_five = tsz_solver::type_queries::create_number_literal_type(&interner, 5.0);
    type_cache
        .node_types
        .insert(var_x_decl.initializer.0, literal_five);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);
    assert!(
        output.contains("declare var x: number;"),
        "Expected mutable generic call literal result to widen in DTS: {output}"
    );
    assert!(
        !output.contains("declare var x: 5;"),
        "Did not expect mutable generic call literal result to stay narrow: {output}"
    );
}

#[test]
fn test_ts_late_bound_function_assignments_ignore_block_scoped_shadow() {
    let source = r#"
export function X() {}
if (Math.random()) {
  const X: { test?: any } = {};
  X.test = 1;
}

export function Y() {}
Y.test = "foo";
if (Math.random()) {
  const Y = function Y() {}
  Y.test = 42;
}
"#;

    let output = emit_dts_with_binding(source);
    let expected = r#"export declare function X(): void;
export declare function Y(): void;
export declare namespace Y {
    var test: string;
}"#;
    assert!(
        output.contains(expected),
        "Expected block-scoped shadow assignments to be ignored: {output}"
    );
}

#[test]
fn test_export_default_function_with_late_bound_assignment_emits_default_alias() {
    let source = r#"
export default function someFunc() {
    return "hello!";
}

someFunc.someProp = "yo";
"#;

    let output = emit_dts_with_usage_analysis(source);
    let expected = r#"declare function someFunc(): string;
declare namespace someFunc {
    var someProp: string;
}
export default someFunc;"#;
    assert!(
        output.contains(expected),
        "Expected default function expandos to emit through a merged namespace alias: {output}"
    );
}

#[test]
fn test_ts_late_bound_function_reserved_alias_avoids_existing_member_name() {
    let source = r#"
export function foo() {}
foo._a = 1;
foo.class = "hello";
"#;

    let output = emit_dts_with_usage_analysis(source);
    let expected = r#"export declare function foo(): void;
export declare namespace foo {
    export var _a: number;
    var _b: string;
    export { _b as class };
}"#;
    assert!(
        output.contains(expected),
        "Synthetic alias for reserved namespace members should skip real member names.\nOutput:\n{output}"
    );
}

#[test]
fn test_js_late_bound_function_reserved_alias_uses_keyword_name() {
    let source = r#"
function foo() {}
foo.null = true;

function bar() {}
bar.async = true;
bar.normal = false;

function baz() {}
baz.class = true;
baz.normal = false;
"#;

    let output = emit_js_dts_with_usage_analysis(source);
    let expected = r#"declare function foo(): void;
declare namespace foo {
    let _null: boolean;
    export { _null as null };
}
declare function bar(): void;
declare namespace bar {
    export let async: boolean;
    export let normal: boolean;
}
declare function baz(): void;
declare namespace baz {
    let _class: boolean;
    export { _class as class };
    let normal_1: boolean;
    export { normal_1 as normal };
}"#;
    assert!(
        output.contains(expected),
        "Expected JS reserved function expandos to use keyword aliases and avoid reused local names.\nOutput:\n{output}"
    );
}

#[test]
fn test_js_late_bound_function_alias_generation_avoids_existing_namespace_members() {
    let source = r#"
export const normal = 1;
export function foo() {}
foo.normal = false;
foo.normal_1 = true;
"#;

    let output = emit_js_dts_with_usage_analysis(source);
    let expected = r#"export function foo(): void;
export namespace foo {
    let normal_2: boolean;
    export { normal_2 as normal };
    let normal_1: boolean;
}"#;
    assert!(
        output.contains(expected),
        "Expected namespace alias generation to skip existing member names when resolving collisions: {output}"
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
fn test_callable_export_expando_function_property_emits_method_signature() {
    let source = r#"
export interface Point {
    readonly x: number;
    readonly y: number;
}

export const Point = (x: number, y: number): Point => ({ x, y });
Point.zero = (): Point => Point(0, 0);
"#;

    let output = emit_dts_with_usage_analysis(source);

    assert!(
        output.contains("zero(): Point;"),
        "Expected function-valued expando on callable export to use method syntax: {output}"
    );
    assert!(
        !output.contains("zero: () => Point;"),
        "Expected not to emit function-valued expando as property syntax: {output}"
    );
}

#[test]
fn test_js_commonjs_exported_arrow_function_preserves_any_return_type() {
    let source = r#"
const donkey = (ast) => ast;
function funky(declaration) { return false; }
module.exports = donkey;
module.exports.funky = funky;
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
        output.contains("declare namespace donkey {\n    export { funky };\n}"),
        "Expected secondary CommonJS function export to merge into the export= namespace: {output}"
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
    assert!(
        !output.contains("declare class K"),
        "Did not expect an intermediate namespace class expando to leak beside the CommonJS export: {output}"
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
fn test_js_commonjs_class_expression_method_body_survives_non_callable_cache() {
    let source = r#"
exports.K = class K {
    values() {
        return new K();
    }
};
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let method_idx = parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            (node.kind == syntax_kind_ext::METHOD_DECLARATION)
                .then_some(NodeIndex(idx as u32))
                .filter(|&method_idx| {
                    parser
                        .arena
                        .get(method_idx)
                        .and_then(|node| parser.arena.get_method_decl(node))
                        .and_then(|method| parser.arena.get_identifier_text(method.name))
                        == Some("values")
                })
        })
        .expect("missing values method");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);
    let interner = TypeInterner::new();
    let mut type_cache = TypeCacheView::default();
    type_cache.node_types.insert(method_idx.0, TypeId::UNKNOWN);
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("values(): K;"),
        "Expected CommonJS class expression method return type to fall back to body inference: {output}"
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
fn test_js_module_exports_new_expression_with_expando_emits_single_let_export() {
    // Regression: `module.exports = new Foo(); module.exports.additional = X;`
    // previously emitted `additional` TWICE — once via the secondary-member
    // path (during `module.exports = new Foo()` emission) and again via the
    // deferred value-export path (when the statement visitor later reached
    // the `module.exports.additional = X` statement). Fix removes the
    // statement from the deferred export maps before secondary emission so
    // the visitor skips it. Also `export const` was emitted where tsc emits
    // `export let` for CommonJS class-instance exports.
    let output = emit_js_dts_with_usage_analysis(
        r#"
class Foo {
    static stat = 10;
    member = 10;
}
module.exports = new Foo();
module.exports.additional = 20;
"#,
    );
    // The `additional` property must appear exactly once in the export
    // surface. (`emit_js_dts_with_usage_analysis` may wrap output in
    // additional preamble lines; the count of literal occurrences is the
    // stable check.)
    let occurrences = output.matches("additional").count();
    assert_eq!(
        occurrences, 1,
        "Expected `additional` to appear exactly once in the .d.ts, got {occurrences}.\nOutput:\n{output}"
    );
    assert!(
        output.contains("export let additional: 20;"),
        "Expected `export let additional: 20;` (CommonJS class-instance exports use `let`): {output}"
    );
    assert!(
        output.contains("export let member: number;"),
        "Expected `export let member: number;` from class instance widening: {output}"
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
fn test_js_exported_object_literal_empty_object_member_emits_namespace_value() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
const x = {
    grey: {}
};
export { x };
"#,
    );

    assert!(
        output.contains("export namespace x {\n    let grey: {};\n}"),
        "Expected named JS object exports with empty object members to emit as namespaces: {output}"
    );
    assert!(
        !output.contains("export const x:"),
        "Did not expect named JS object exports with empty object members to fall back to const object types: {output}"
    );
}

#[test]
fn test_js_commonjs_named_object_alias_empty_object_member_emits_namespace_value() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
const chalk = {
    grey: {}
};
module.exports.chalk = chalk;
"#,
    );

    assert!(
        output.contains("export namespace chalk {\n    let grey: {};\n}"),
        "Expected CommonJS named object aliases with empty object members to emit as namespaces: {output}"
    );
    assert!(
        !output.contains("export const chalk:"),
        "Did not expect CommonJS named object aliases with empty object members to fall back to const object types: {output}"
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
fn test_js_module_exports_anonymous_class_secondary_class_emits_once() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
module.exports = class {
    constructor(p) {
        this.t = 12 + p;
    }
};
module.exports.Sub = class {
    constructor() {
        this.instance = new module.exports(10);
    }
};
"#,
    );

    assert!(
        output.contains("declare namespace exports {\n    export { Sub };\n}"),
        "Expected secondary anonymous class exports to be aliased through the export= namespace: {output}"
    );
    assert!(
        output.contains("declare class Sub {"),
        "Expected secondary anonymous class exports to emit a local class declaration: {output}"
    );
    assert!(
        !output.contains("export class Sub"),
        "Did not expect the secondary class assignment to also emit as a named export: {output}"
    );
}

#[test]
fn test_js_commonjs_constructor_function_prototype_object_emits_single_class() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/** @constructor */
module.exports.MyClass = function() {
    this.x = 1;
};
module.exports.MyClass.prototype = {
    a: function() {
    }
};
"#,
    );

    assert!(
        output.contains("export class MyClass {\n    a: () => void;\n}"),
        "Expected CommonJS constructor functions with prototype object literals to emit as a single class surface: {output}"
    );
    assert!(
        !output.contains("export function MyClass"),
        "Did not expect the constructor function assignment to emit beside the class: {output}"
    );
    assert!(
        !output.contains("constructor();") && !output.contains("x: number;"),
        "Did not expect constructor-body properties to leak when tsc uses the prototype object surface: {output}"
    );
}

#[test]
fn test_js_commonjs_export_assignment_inside_closure_emits_export_surface() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
function foo() {
    module.exports = exports = function (o) {
        return o;
    };
    const m = function () {
    }
    exports.methods = m;
}
"#,
    );

    assert!(
        output.contains("declare function _exports(o: any): any;"),
        "Expected closure CommonJS root assignment to emit export= function surface: {output}"
    );
    assert!(
        output.contains("declare namespace _exports {\n    export { m as methods };\n}"),
        "Expected closure CommonJS secondary exports to attach to the synthetic namespace: {output}"
    );
    assert!(
        output.contains("export = _exports;\ndeclare function m(): void;"),
        "Expected local function secondary export target to be emitted after export=: {output}"
    );
    assert!(
        !output.contains("declare function foo"),
        "Did not expect enclosing helper closure to leak as the declaration surface: {output}"
    );
}

#[test]
fn test_jsdoc_type_tags_on_const_null_preserve_closure_type_syntax() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/** @type {?} */
export const a = null;
/** @type {*} */
export const b = null;
/** @type {string?} */
export const c = null;
/** @type {string=} */
export const d = null;
/** @type {string!} */
export const e = null;
/** @type {function(string, number): object} */
export const f = null;
/** @type {function(new: object, string, number)} */
export const g = null;
/** @type {Object.<string, number>} */
export const h = null;
"#,
    );

    assert!(
        output.contains("export const a: unknown;"),
        "Expected bare Closure unknown @type to win over const null fallback: {output}"
    );
    assert!(output.contains("export const b: any;"));
    assert!(output.contains("export const c: string | null;"));
    assert!(output.contains("export const d: string | undefined;"));
    assert!(output.contains("export const e: string;"));
    assert!(output.contains("export const f: (arg0: string, arg1: number) => object;"));
    assert!(output.contains("export const g: new (arg1: string, arg2: number) => object;"));
    assert!(output.contains("export const h: {\n    [x: string]: number;\n};"));
}

#[test]
fn test_jsdoc_typedef_comment_before_namespace_object_is_not_duplicated() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @template T
 * @template {keyof T} K
 * @typedef {T[K]} Foo
 */
const x = { a: 1 };

/** @type {Foo<typeof x, "a">} */
const y = "a";
"#,
    );

    assert!(
        output.starts_with("declare namespace x {\n    let a: number;\n}"),
        "Expected namespace object emit without leaking implementation-only typedef JSDoc: {output}"
    );
    assert!(
        output.contains("type Foo<T, K extends keyof T> = T[K];"),
        "Expected typedef alias to still be emitted: {output}"
    );
    assert!(
        !output.contains("@typedef"),
        "Did not expect the source typedef comment to be duplicated in the DTS: {output}"
    );
}

#[test]
fn test_js_array_subclass_emits_array_any_and_constructors() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
class ElementsArray extends Array {
    static {
        this.isArray = (arg) => Array.isArray(arg);
    }
}
"#,
    );

    let expected = "declare class ElementsArray extends Array<any> {\n    constructor(arrayLength?: number);\n    constructor(arrayLength: number);\n    constructor(...items: any[]);\n}";
    assert!(
        output.contains(expected),
        "Expected bare JS Array subclasses to inherit Array constructor overloads: {output}"
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
fn test_namespace_exported_proto_var_suppresses_private_interface_merge() {
    let output = emit_dts_with_usage_analysis(
        r#"
namespace m1 {
    export var __proto__;
    interface __proto__ {}

    class C<T extends { __proto__: __proto__ }> { }
}
__proto__ = 0;
m1.__proto__ = 0;
"#,
    );

    assert!(
        output.contains("declare namespace m1 {\n    var __proto__: any;\n}"),
        "Expected exported __proto__ var to stay as the namespace surface: {output}"
    );
    assert!(
        !output.contains("interface __proto__"),
        "Private merged __proto__ interface should not leak into the namespace d.ts: {output}"
    );
    assert!(
        !output.contains("export {};"),
        "Skipping the private interface should also avoid a namespace scope marker: {output}"
    );
}

#[test]
fn test_namespace_exported_proto_interface_is_public_surface() {
    let output = emit_dts_with_usage_analysis(
        r#"
namespace m1 {
    export interface __proto__ {
        value: string;
    }
}
"#,
    );

    assert!(
        output.contains("interface __proto__"),
        "Expected exported __proto__ interface to stay in namespace d.ts: {output}"
    );
    assert!(
        output.contains("value: string;"),
        "Expected exported __proto__ interface members to stay in namespace d.ts: {output}"
    );
}

#[test]
fn test_js_class_getter_before_setter_preserves_both_accessors() {
    let output = emit_js_dts(
        r#"
class C {
    /** @returns {number} */
    get value() {
        return 1;
    }
    /** @param {number} next */
    set value(next) {
    }
}
"#,
    );

    let setter_pos = output
        .find("set value(next: number);")
        .expect("missing setter in output");
    let getter_pos = output
        .find("get value(): number;")
        .expect("missing getter in output");

    assert!(
        setter_pos < getter_pos,
        "Expected setter/getter pair to be emitted together even when getter appears first: {output}"
    );
}

#[test]
fn test_js_class_define_property_prototype_accessors_emit() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
export class D {}
Object.defineProperty(D.prototype, "x", {
    get() {
        return 12;
    },
    /** @param {number} _arg */
    set(_arg) {}
});

/** @param {number} v */
const setter = (v) => {};
export class E {}
Object.defineProperty(E.prototype, "x", { set: setter });
"#,
    );

    assert!(
        output.contains("export class D {\n    set x(_arg: number);\n    get x(): number;\n}"),
        "Expected descriptor getter/setter to fold into class D: {output}"
    );
    assert!(
        output.contains("export class E {\n    set x(value: number);\n}"),
        "Expected descriptor setter alias to fold into class E: {output}"
    );
    assert!(
        !output.contains("Object.defineProperty"),
        "Descriptor statements should not leak to declaration output: {output}"
    );
}

#[test]
fn test_js_named_export_equals_class_expression_shadowing_preserves_root_name() {
    let output = emit_js_dts_with_usage_analysis(
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
        output.contains("declare class A {\n    member: Q;\n}"),
        "Expected local classes referenced by the exported class expression surface to be retained: {output}"
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
fn test_js_class_property_type_resolves_semicolon_typedef_alias() {
    let output = emit_js_dts(
        r#"
export class Box {
    /** @typedef {{ id: string }} Prop */
    ;
    /** @type {Prop} */
    value;
}
"#,
    );

    assert!(
        output.contains("value: { id: string };"),
        "Expected class property JSDoc @type alias to resolve from nearby semicolon-only typedef: {output}"
    );
    assert!(
        !output.contains("value: Prop;"),
        "Expected class property type to emit resolved typedef body, not unresolved alias name: {output}"
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
fn test_jsdoc_property_typedef_quotes_non_identifier_names() {
    let source = r#"
/**
 * @typedef {Object} Options
 * @property {String} data-id
 * @property {Number} [max-count]
 */
exports.value = {};
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("\"data-id\": string;"),
        "Expected hyphenated JSDoc property name to be quoted: {output}"
    );
    assert!(
        output.contains("\"max-count\"?: number;"),
        "Expected optional hyphenated JSDoc property name to be quoted before ?: {output}"
    );
}

#[test]
fn test_jsdoc_property_typedef_preserves_alias_description() {
    let source = r#"
/**
 * Options for Foo.
 * @typedef {Object} FooOptions
 * @property {boolean} bar - Enables bar.
 */
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("/**\n * Options for Foo.\n */\ntype FooOptions = {"),
        "Expected typedef description to be preserved above the type alias: {output}"
    );
    assert!(
        output.contains("/**\n     * - Enables bar.\n     */\n    bar: boolean;"),
        "Expected property description to remain on the property: {output}"
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
    let (parser, root) = parse_test_source(source);

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
    let (parser, root) = parse_test_source(source);

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
    let (parser, root) = parse_test_source(source);

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
fn test_private_overloaded_method_initializer_reuses_matching_signature_return_type() {
    let output = emit_dts_with_binding(
        r#"
function noArgs(): string { return null as any; }
function oneArg(input: string): string { return null as any; }

export class Wrapper {
    private proxy<T, U>(fn: (options: T) => U): (options: T) => U;
    private proxy<T, U>(fn: (options?: T) => U, noArgs: true): (options?: T) => U;
    private proxy<T, U>(fn: (options: T) => U) {
        return null as any;
    }

    public Proxies = {
        Failure: this.proxy(noArgs, true),
        Success: this.proxy(oneArg),
    };
}
"#,
    );

    assert!(
        output.contains("Failure: (options?: unknown) => string;"),
        "Expected optional proxy overload to infer a callable return type: {output}"
    );
    assert!(
        output.contains("Success: (options: string) => string;"),
        "Expected one-argument proxy overload to infer a callable return type: {output}"
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
fn symbol_observer_computed_member_drops_redundant_index_signature() {
    let source = r#"
interface SymbolConstructor {
    readonly observer: symbol;
}
interface SymbolConstructor {
    readonly observer: unique symbol;
}

const obj = {
    [Symbol.observer]: 0
};
"#;
    let (parser, root) = parse_test_source(source);

    let obj_decl = parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            parser
                .arena
                .get_variable_declaration(node)
                .filter(|decl| parser.arena.get_identifier_text(decl.name) == Some("obj"))
                .map(|decl| (NodeIndex(idx as u32), decl))
        })
        .map(|(_, decl)| decl)
        .expect("missing obj declaration");
    let object_literal = parser
        .arena
        .get(obj_decl.initializer)
        .and_then(|node| parser.arena.get_literal_expr(node))
        .expect("missing obj object literal");
    let prop_assignment = parser
        .arena
        .get(object_literal.elements.nodes[0])
        .and_then(|node| parser.arena.get_property_assignment(node))
        .expect("missing computed property assignment");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let object_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: Some(interner.intern_string("x")),
        }),
        symbol: None,
    });

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache
        .node_types
        .insert(obj_decl.initializer.0, object_type);
    type_cache
        .node_types
        .insert(prop_assignment.initializer.0, TypeId::NUMBER);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("[Symbol.observer]: number;"),
        "Expected computed symbol property to survive: {output}"
    );
    assert!(
        !output.contains("[x: number]: number;"),
        "Did not expect redundant synthetic numeric index signature: {output}"
    );
}

#[test]
fn non_symbol_computed_member_preserves_matching_index_signature() {
    let source = r#"
const key = "x";
const obj = {
    [key]: 0
};
"#;
    let (parser, root) = parse_test_source(source);

    let obj_decl = parser
        .arena
        .nodes
        .iter()
        .find_map(|node| {
            parser
                .arena
                .get_variable_declaration(node)
                .filter(|decl| parser.arena.get_identifier_text(decl.name) == Some("obj"))
        })
        .expect("missing obj declaration");
    let object_literal = parser
        .arena
        .get(obj_decl.initializer)
        .and_then(|node| parser.arena.get_literal_expr(node))
        .expect("missing obj object literal");
    let prop_assignment = parser
        .arena
        .get(object_literal.elements.nodes[0])
        .and_then(|node| parser.arena.get_property_assignment(node))
        .expect("missing computed property assignment");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let object_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: Some(interner.intern_string("x")),
        }),
        symbol: None,
    });

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache
        .node_types
        .insert(obj_decl.initializer.0, object_type);
    type_cache
        .node_types
        .insert(prop_assignment.initializer.0, TypeId::NUMBER);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("[x: number]: number;"),
        "Expected non-Symbol computed property to preserve matching index signature: {output}"
    );
}

#[test]
fn well_known_symbol_computed_member_preserves_matching_index_signature() {
    let source = r#"
const obj = {
    [Symbol.iterator]: 0
};
"#;
    let (parser, root) = parse_test_source(source);

    let obj_decl = parser
        .arena
        .nodes
        .iter()
        .find_map(|node| {
            parser
                .arena
                .get_variable_declaration(node)
                .filter(|decl| parser.arena.get_identifier_text(decl.name) == Some("obj"))
        })
        .expect("missing obj declaration");
    let object_literal = parser
        .arena
        .get(obj_decl.initializer)
        .and_then(|node| parser.arena.get_literal_expr(node))
        .expect("missing obj object literal");
    let prop_assignment = parser
        .arena
        .get(object_literal.elements.nodes[0])
        .and_then(|node| parser.arena.get_property_assignment(node))
        .expect("missing computed property assignment");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let object_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: Some(interner.intern_string("x")),
        }),
        symbol: None,
    });

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache
        .node_types
        .insert(obj_decl.initializer.0, object_type);
    type_cache
        .node_types
        .insert(prop_assignment.initializer.0, TypeId::NUMBER);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("[Symbol.iterator]: number;"),
        "Expected computed symbol property to survive: {output}"
    );
    assert!(
        output.contains("[x: number]: number;"),
        "Expected non-observer Symbol computed property to preserve matching index signature: {output}"
    );
}

#[test]
fn negative_numeric_computed_member_preserves_computed_syntax() {
    let output = emit_dts_with_usage_analysis(
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
        "Expected negative numeric computed syntax to be preserved: {output}"
    );
    assert!(
        !output.contains("\"-1\": {};"),
        "Did not expect negative numeric computed property to be quoted: {output}"
    );
    assert!(
        !output.contains("[-2]: {};"),
        "Expected non-literal negative numeric key to be covered by the numeric index signature: {output}"
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
    let (parser, root) = parse_test_source(source);

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
fn test_local_interface_computed_names_do_not_leak_public_dependencies() {
    let output = emit_dts_with_usage_analysis(
        r#"
const localStringKey = "local";
const localNumberKey = 1;
const publicSymbolKey = Symbol();

interface LocalStringNamed {
    [localStringKey]: number;
}

interface LocalNumberNamed {
    [localNumberKey]: string;
}

export interface PublicNamed {
    [publicSymbolKey]: number;
}
"#,
    );

    assert!(
        !output.contains("localStringKey"),
        "Did not expect local-only interface computed name dependencies to emit: {output}"
    );
    assert!(
        !output.contains("localNumberKey"),
        "Did not expect local-only interface computed name dependencies to emit: {output}"
    );
    assert!(
        output.contains("declare const publicSymbolKey"),
        "Expected exported interface computed name dependency to emit: {output}"
    );
    assert!(
        output.contains("[publicSymbolKey]: number;"),
        "Expected exported interface computed member to emit: {output}"
    );
}

#[test]
fn test_referenced_local_interface_computed_names_keep_dependencies() {
    let output = emit_dts_with_usage_analysis(
        r#"
const localSymbolKey = Symbol();

interface LocalNamed {
    [localSymbolKey]: number;
}

export interface PublicNamed extends LocalNamed {}
"#,
    );

    assert!(
        output.contains("declare const localSymbolKey"),
        "Expected local interface computed name dependency to emit when interface is public: {output}"
    );
    assert!(
        output.contains("interface LocalNamed"),
        "Expected referenced local interface to emit: {output}"
    );
    assert!(
        output.contains("[localSymbolKey]: number;"),
        "Expected referenced local interface computed member to emit: {output}"
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
fn test_class_property_initializer_same_name_enum_uses_typeof_enum() {
    let source = r#"
enum Hello {
    World
}
class Foo {
    Hello = Hello;
}
"#;
    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);
    let source_file = parser
        .arena
        .get(root)
        .and_then(|node| parser.arena.get_source_file(node))
        .expect("missing source file");
    let class_idx = source_file.statements.nodes[1];
    let prop_idx = parser
        .arena
        .get(class_idx)
        .and_then(|node| parser.arena.get_class(node))
        .and_then(|class| class.members.nodes.first().copied())
        .expect("missing property");

    let interner = TypeInterner::new();
    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(prop_idx.0, TypeId::ANY);
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("Hello: typeof Hello;"),
        "Expected same-name enum initializer to emit typeof enum: {output}"
    );
    assert!(
        !output.contains("readonly [x: number]"),
        "Did not expect enum value object shape to leak into property type: {output}"
    );
}

#[test]
fn test_class_property_initializer_same_name_enum_uses_typeof_with_inferred_shape() {
    let output = emit_dts_with_binding(
        r#"
enum Hello {
    World
}
class Foo {
    Hello = Hello;
}
"#,
    );

    assert!(
        output.contains("Hello: typeof Hello;"),
        "Expected same-name enum initializer to emit typeof enum: {output}"
    );
    assert!(
        !output.contains("readonly [x: number]"),
        "Did not expect enum value object shape to leak into property type: {output}"
    );
}

#[test]
fn test_returned_local_conditional_annotation_uses_function_generic_scope() {
    let output = emit_dts_with_binding(
        r#"
function g<T>(x: T) {
    let y: typeof x extends (infer T)[] ? T : typeof x = null as any;
    return y;
}
"#,
    );

    assert!(
        output.contains("declare function g<T>(x: T): T extends (infer T_1)[] ? T_1 : T;"),
        "Expected returned local annotation to substitute parameter type queries and rename shadowed infer type parameter: {output}"
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
fn test_inferred_const_from_namespace_infinity_alias_emits_literal() {
    let output = emit_dts_with_binding(
        r#"
export enum Foo {
    A = 1e999,
    B = -1e999,
}

namespace X {
    type A = 1e999;

    export function f(): A {
        throw new Error()
    }
}

export const m = X.f();
"#,
    );

    assert!(
        output.contains("export declare const m: Infinity;"),
        "Expected inaccessible infinity alias return to emit structural literal: {output}"
    );
    assert!(
        !output.contains("export declare const m: A;"),
        "Did not expect inaccessible alias or unqualified enum member to leak: {output}"
    );
}

#[test]
fn test_inferred_const_from_explicit_enum_member_return_keeps_member_type() {
    let output = emit_dts_with_binding(
        r#"
export enum Foo {
    A = 1,
    B = 2,
}

namespace X {
    export function f(): Foo.A {
        throw new Error()
    }
}

export const m = X.f();
"#,
    );

    assert!(
        output.contains("export declare const m: Foo.A;"),
        "Expected explicit enum member return annotation to stay nameable: {output}"
    );
    assert!(
        !output.contains("export declare const m: 1;"),
        "Did not expect explicit enum member return to collapse to literal: {output}"
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
    let (parser, root) = parse_test_source(source);

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
    let (parser, root) = parse_test_source(source);

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
    let (parser, root) = parse_test_source(source);

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
    let (parser, root) = parse_test_source(source);

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
    let (parser, root) = parse_test_source(source);

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
    let (parser, root) = parse_test_source(source);

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
    let (parser, root) = parse_test_source(source);

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
    let (parser, root) = parse_test_source(source);

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
fn test_redundant_named_import_alias_extends_uses_canonical_name() {
    let output = emit_dts_with_usage_analysis(
        r#"
import { Base, Base as Base2 } from "pkg";
export class A extends Base {}
export class B extends Base2 {}
"#,
    );

    assert!(
        output.contains("export declare class A extends Base"),
        "Expected first class to keep canonical import name: {output}"
    );
    assert!(
        output.contains("export declare class B extends Base"),
        "Expected aliased class heritage to use canonical import name: {output}"
    );
    assert!(
        output.contains("import { Base } from \"pkg\";"),
        "Expected redundant named import alias to be elided: {output}"
    );
    assert!(
        !output.contains("Base2"),
        "Did not expect declaration output to reference redundant alias: {output}"
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
fn test_js_default_typedef_after_default_identifier_export_uses_export_name() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
class Cls {
    x = 12;
}
export default Cls;
/** @typedef {string | number} default */
"#,
    );
    let trimmed = output.trim();
    assert!(
        trimmed.starts_with("export type Cls = string | number;\nexport default Cls;"),
        "Expected default typedef to reuse the default-exported class name before the hoisted default export: {trimmed}"
    );
    assert!(
        !trimmed.contains("export type Cls_1 = string | number;"),
        "Default typedef alias should not synthesize a unique name for a default-exported class: {trimmed}"
    );
    assert!(
        trimmed.contains("declare class Cls"),
        "Expected the exported class declaration to remain: {trimmed}"
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

/// Regression: a TypeScript class whose computed-name members appear
/// *before* the constructor must keep that order in d.ts.  Prior code
/// hoisted the constructor between statics and instance members
/// whenever a class had any computed name, which mangled
/// `[a]: number; [b]: number; constructor();` into
/// `constructor(); [a]: number; [b]: number;`.  tsc preserves source
/// order here (statics still hoist, but the constructor stays in its
/// non-static slot).
#[test]
fn ts_class_with_computed_names_keeps_constructor_after_instance_members() {
    let output = emit_dts(
        r#"
declare const a: 'a';
declare const b: unique symbol;
class C12 {
    [a]: number;
    [b]: number;
    ['c']: number;
    constructor() {}
}
"#,
    );
    let trimmed = output.trim();
    let a_pos = trimmed.find("[a]: number;").expect("expected [a] member");
    let b_pos = trimmed.find("[b]: number;").expect("expected [b] member");
    let c_pos = trimmed
        .find("['c']: number;")
        .expect("expected ['c'] member");
    let ctor_pos = trimmed
        .find("constructor();")
        .expect("expected constructor declaration");
    assert!(
        a_pos < b_pos && b_pos < c_pos && c_pos < ctor_pos,
        "TS class with computed names should preserve source order — instance members before constructor: {trimmed}"
    );
}

/// Regression: a `TupleType` whose source has JSDoc comments preceding
/// individual members must round-trip in d.ts emit as a multi-line
/// tuple with each comment on its own line, mirroring tsc's behaviour
/// (see `namedTupleMembers.SegmentAnnotated`).
///
/// Counter-regression: tuples *without* leading JSDoc on any member
/// must keep the compact one-line shape — the multi-line switch is
/// JSDoc-only, not "any time we have named tuple members" or "any
/// time we have a rest element".
#[test]
fn ts_tuple_with_jsdoc_member_emits_multiline_with_comments() {
    let output = emit_dts(
        r#"
export type SegmentAnnotated = [
    /**
     * Size of message buffer segment handles
     */
    length: number,
    /**
     * Number of segments handled at once
     */
    count: number
];
"#,
    );
    assert!(
        output.contains("/**\n     * Size of message buffer segment handles\n     */"),
        "tuple-member JSDoc should round-trip in d.ts emit: {output}"
    );
    assert!(
        output.contains("/**\n     * Number of segments handled at once\n     */"),
        "second tuple-member JSDoc should round-trip too: {output}"
    );
    let length_idx = output.find("length: number").expect("length member");
    let count_idx = output.find("count: number").expect("count member");
    assert!(
        length_idx < count_idx,
        "tuple member order must be preserved: {output}"
    );
}

#[test]
fn ts_tuple_without_jsdoc_member_keeps_single_line_form() {
    let output = emit_dts(
        r#"
export type Segment = [length: number, count: number];
export type WithRest = [first: number, second?: number, ...rest: string[]];
"#,
    );
    let trimmed = output.trim();
    assert!(
        trimmed
            .lines()
            .any(|l| l.contains("export type Segment = [length: number, count: number];")),
        "non-annotated tuple should stay single-line: {output}"
    );
    assert!(
        trimmed.lines().any(|l| l.contains(
            "export type WithRest = [first: number, second?: number, ...rest: string[]];"
        )),
        "rest-only tuple without JSDoc should stay single-line: {output}"
    );
}

/// Counter-regression: when computed-named instance members appear in
/// source order *before* static members, the static members must still
/// hoist to the top of the d.ts class body — that's the actual rule
/// tsc follows for computed-name TS classes (see
/// `declarationEmitSimpleComputedNames1`).  Verifies the static-hoist
/// rule didn't regress when the constructor-handling fix landed.
#[test]
fn ts_class_with_computed_names_hoists_static_members_above_instance() {
    let output = emit_dts(
        r#"
declare const classFieldName: string;
declare const otherField: string;
declare const staticField: string;
export class Holder {
    [classFieldName]() { return "value"; }
    [otherField]() { return 42; }
    static [staticField]() { return { static: true as boolean }; }
    static [staticField]() { return { static: "sometimes" as string }; }
}
"#,
    );
    let trimmed = output.trim();
    let static_a = trimmed
        .find("static [staticField]")
        .expect("expected first static member");
    let instance_a = trimmed
        .find("[classFieldName]")
        .expect("expected first instance member");
    let instance_b = trimmed
        .find("[otherField]")
        .expect("expected second instance member");
    assert!(
        static_a < instance_a && static_a < instance_b,
        "static members should hoist above instance members for TS classes with computed names: {trimmed}"
    );
}

/// Direct regression test for the trim helper used by
/// `type_argument_list_source_text`.  Two-axis property: a bare
/// overshoot `Foo>` becomes `Foo`, and a nested balanced `<…>` like
/// `C.A<C.B>` is left intact (naive trimming would corrupt it into
/// `C.A<C.B`).  The parser's `token_full_start()` correctly anchors
/// `TypeReference` ends; only `LiteralType`/`UnionType`/
/// `IntersectionType` have the `token_end()` overshoot quirk this
/// helper fixes.
#[test]
fn strip_type_argument_overshoot_balances_nested_angle_brackets() {
    use crate::declaration_emitter::DeclarationEmitter;

    let mut overshoot = String::from("\"Hello\">");
    DeclarationEmitter::strip_type_argument_overshoot_for_test(&mut overshoot);
    assert_eq!(
        overshoot, "\"Hello\"",
        "literal-type overshoot must be trimmed"
    );

    let mut nested = String::from("C.A<C.B>");
    DeclarationEmitter::strip_type_argument_overshoot_for_test(&mut nested);
    assert_eq!(
        nested, "C.A<C.B>",
        "balanced nested `<…>` must not be trimmed"
    );

    let mut nested_with_overshoot = String::from("C.A<C.B>>");
    DeclarationEmitter::strip_type_argument_overshoot_for_test(&mut nested_with_overshoot);
    assert_eq!(
        nested_with_overshoot, "C.A<C.B>",
        "trailing overshoot `>` must be trimmed but inner `>` kept"
    );

    let mut trailing_comma = String::from("\"foo\", ");
    DeclarationEmitter::strip_type_argument_overshoot_for_test(&mut trailing_comma);
    assert_eq!(
        trailing_comma, "\"foo\"",
        "trailing `,`/whitespace must drop"
    );

    let mut quoted_gt = String::from("\"a>b\"");
    DeclarationEmitter::strip_type_argument_overshoot_for_test(&mut quoted_gt);
    assert_eq!(
        quoted_gt, "\"a>b\"",
        "`>` inside string literals must not affect the balance count"
    );
}

#[test]
fn test_js_exported_class_emits_documented_constructor_assignment_field() {
    let source = r#"
export class Aleph {
    /**
     * Impossible to construct.
     * @param {Aleph} a
     * @param {null} b
     */
	    constructor(a, b) {
	        /**
	         * Field is always null
	         */
	        this.field = b;
	        /**
	         * Explicitly typed count.
	         * @type {number}
	         */
	        this.count = 1;
	    }

    /**
     * Doesn't actually do anything
     * @returns {void}
     */
    doIt() {}
	}
	"#;
    let output = emit_js_dts(source);

    assert!(
        output.contains(
            "/**\n     * Field is always null\n     */\n    field: any;\n    /**\n     * Explicitly typed count.\n     * @type {number}\n     */\n    count: number;\n    /**\n     * Doesn't actually do anything"
        ),
        "Expected documented constructor assignment field before method declaration: {output}"
    );
}

#[test]
fn test_js_local_bare_require_alias_without_exports_is_elided() {
    let source = r#"
const u = require("untyped");
u.assignment.nested = true;
u.noError();
"#;
    let output = emit_js_dts(source);

    assert!(
        !output.contains("declare const u"),
        "Expected local bare require alias in a non-exporting JS module to be elided: {output}"
    );
    assert_eq!("export {};", output.trim());
}

#[test]
fn test_js_local_dynamic_require_alias_without_exports_is_preserved() {
    let source = r#"
const moduleName = "untyped";
const u = require(moduleName);
u.noError();
"#;
    let output = emit_js_dts(source);

    assert!(
        output.contains("declare const u: any;"),
        "Expected dynamic require alias to be preserved: {output}"
    );
}

#[test]
fn test_js_returned_function_expression_uses_attached_jsdoc_signature() {
    let output = emit_js_dts(
        r#"
function f1() {
    /**
     * @param {number} a
     * @param {number} b
     * @returns {number}
     */
    return (a, b) => a + b;
}

function f2() {
    /** @type {(a: string, b: string) => string} */
    return function (a, b) {
        return a + b;
    };
}
"#,
    );

    assert!(
        output.contains("declare function f1(): (a: number, b: number) => number;"),
        "Expected returned arrow signature to use attached @param/@returns JSDoc: {output}"
    );
    assert!(
        output.contains("declare function f2(): (a: string, b: string) => string;"),
        "Expected returned function expression signature to use attached @type JSDoc: {output}"
    );
}

#[test]
fn test_js_export_equals_function_static_assignments_stay_top_level() {
    let output = emit_js_dts(
        r#"
module.exports = MyClass;

function MyClass() {}
MyClass.staticMethod = function() {}
MyClass.prototype.method = function() {}
MyClass.staticProperty = 123;
"#,
    );

    assert!(
        output.contains("export = MyClass;"),
        "Expected CommonJS export assignment: {output}"
    );
    assert!(
        output.contains(
            "declare namespace MyClass {\n    export { staticMethod, staticProperty };\n}"
        ),
        "Expected namespace to re-export top-level expando declarations: {output}"
    );
    assert!(
        output.contains("declare function staticMethod(): void;"),
        "Expected static function expando to remain a top-level declaration: {output}"
    );
    assert!(
        output.contains("declare var staticProperty: number;"),
        "Expected static value expando to remain a top-level declaration: {output}"
    );
    assert!(
        !output.contains("declare namespace MyClass {\n    function staticMethod(): void;"),
        "Did not expect static expandos to be folded into the namespace body: {output}"
    );
}

#[test]
fn test_js_function_static_properties_export_from_merged_namespace() {
    let output = emit_js_dts(
        r#"
function foo() {}
foo.x = 1;
foo.default = 2;
"#,
    );

    assert!(
        output.contains("declare namespace foo {\n    let x: number;"),
        "Expected ordinary expando property to emit as an ambient namespace member: {output}"
    );
    assert!(
        output.contains("let _default: number;\n    export { _default as default };"),
        "Expected reserved expando property to use local alias plus export specifier: {output}"
    );
}

#[test]
fn test_js_commonjs_factory_namespace_alias_declaration_emits_after_namespace() {
    let output = emit_js_dts(
        r#"
class Base {
    constructor() {}
}

const BaseFactory = () => {
    return new Base();
};

BaseFactory.Base = Base;
module.exports = BaseFactory;
"#,
    );

    let export_pos = output
        .find("export = BaseFactory;")
        .expect("Expected CommonJS export assignment");
    let factory_pos = output
        .find("declare function BaseFactory")
        .expect("Expected factory function declaration");
    let namespace_pos = output
        .find("declare namespace BaseFactory")
        .expect("Expected merged namespace declaration");
    let class_pos = output
        .find("declare class Base")
        .expect("Expected local class dependency declaration");

    assert!(
        export_pos < factory_pos && factory_pos < namespace_pos && namespace_pos < class_pos,
        "Expected namespace alias dependency declaration to follow the namespace schedule: {output}"
    );
    assert!(
        output.contains("export { Base };"),
        "Expected namespace to export the local class alias: {output}"
    );
}

#[test]
fn test_js_commonjs_namespace_alias_jsdoc_function_declaration_emits_once_after_namespace() {
    let output = emit_js_dts(
        r#"
function Root() {}

/**
 * @param {number} x
 * @returns {number}
 */
function Member(x) {
    return x;
}

Root.Member = Member;
module.exports = Root;
"#,
    );

    let namespace_pos = output
        .find("declare namespace Root")
        .expect("Expected merged namespace declaration");
    let member_pos = output
        .find("declare function Member")
        .expect("Expected local function dependency declaration");

    assert!(
        namespace_pos < member_pos,
        "Expected JSDoc alias dependency declaration to follow the namespace schedule: {output}"
    );
    assert_eq!(
        output.matches("declare function Member").count(),
        1,
        "Expected JSDoc alias dependency declaration to emit once: {output}"
    );
    assert!(
        output.contains("export { Member };"),
        "Expected namespace to export the local function alias: {output}"
    );
}

#[test]
fn test_js_commonjs_expando_does_not_defer_unrelated_same_named_jsdoc_function() {
    let output = emit_js_dts(
        r#"
function Root() {}

/**
 * @returns {string}
 */
function x() {
    return "";
}

Root.x = 1;
module.exports = Root;
"#,
    );

    let function_pos = output
        .find("declare function x")
        .expect("Expected unrelated same-named function declaration");
    let namespace_pos = output
        .find("declare namespace Root")
        .expect("Expected merged namespace declaration");

    assert!(
        function_pos < namespace_pos,
        "Expected same-named non-alias JSDoc function to avoid namespace-alias deferral: {output}"
    );
    assert!(
        output.contains("declare var x: number;"),
        "Expected non-alias expando property declaration to remain a value declaration: {output}"
    );
}

#[test]
fn test_js_reordered_accessor_comments_keep_backing_field_comment() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
export const key = Symbol("key");

export class C {
    /**
     * @protected
     * @type {null | string}
     */
    [key] = null;

    get value() {
        return this[key];
    }

    /**
     * @type {string}
     */
    set value(v) {
        this[key] = v;
    }
}
"#,
    );

    assert!(
        output.contains(" * @type {string}\n     */\n    set value(v: string | null);"),
        "Expected setter to keep its own JSDoc and backing nullability: {output}"
    );
    assert!(
        output.contains("get value(): string | null;"),
        "Expected getter to reuse the setter/backing-field type: {output}"
    );
    assert!(
        output.contains(
            " * @protected\n     * @type {null | string}\n     */\n    protected [key]: null | string;"
        ),
        "Expected backing field JSDoc to stay attached to the deferred field: {output}"
    );
}

#[test]
fn test_js_getter_uses_jsdoc_type_tag() {
    let output = emit_js_dts(
        r#"
class C {
    /** @type {string=} */
    get p1() {
        return undefined;
    }

    /** @type {?string} */
    get p2() {
        return null;
    }

    /** @type {string | null} */
    get p3() {
        return null;
    }
}
"#,
    );

    assert!(
        output.contains("get p1(): string | undefined;"),
        "Expected getter @type to override undefined body inference: {output}"
    );
    assert!(
        output.contains("get p2(): string | null;"),
        "Expected nullable getter @type to override null body inference: {output}"
    );
    assert!(
        output.contains("get p3(): string | null;"),
        "Expected explicit union getter @type to override null body inference: {output}"
    );
}

#[test]
fn test_js_accessor_pair_preserves_jsdoc_type_comments_and_optional_param_type() {
    let output = emit_js_dts(
        r#"
class C {
    /** @type {string=} */
    get value() {
        return undefined;
    }

    /** @param {string=} value */
    set value(value) {
        this.value = value;
    }
}
"#,
    );

    assert!(
        output.contains(
            "    /** @param {string=} value */\n    set value(value: string | undefined);"
        ),
        "Expected reordered setter comment to stay single-line and optional param to emit as a union: {output}"
    );
    assert!(
        output.contains("    /** @type {string=} */\n    get value(): string | undefined;"),
        "Expected reordered getter comment to stay single-line and @type to drive getter type: {output}"
    );
}

#[test]
fn test_js_accessor_pair_preserves_multiline_jsdoc_type_comments_when_reordered() {
    let output = emit_js_dts(
        r#"
class C {
    /**
     * @type {string=}
     */
    get value() {
        return undefined;
    }

    /**
     * @param {string=} value
     */
    set value(value) {
        this.value = value;
    }
}
"#,
    );

    assert!(
        output.contains("    /**\n     * @param {string=} value\n     */\n    set value(value: string | undefined);"),
        "Expected reordered setter comment to stay multiline: {output}"
    );
    assert!(
        output.contains(
            "    /**\n     * @type {string=}\n     */\n    get value(): string | undefined;"
        ),
        "Expected reordered getter comment to stay multiline: {output}"
    );
    assert!(
        !output.contains("/** @param {string=} value */\n    set value"),
        "Did not expect reordered setter comment to collapse to one line: {output}"
    );
    assert!(
        !output.contains("/** @type {string=} */\n    get value"),
        "Did not expect reordered getter comment to collapse to one line: {output}"
    );
}

#[test]
fn test_js_setter_does_not_lift_nested_nullish_from_array_element_union() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
export const key = Symbol("key");

export class C {
    /**
     * @protected
     * @type {(null | string)[]}
     */
    [key] = [];

    /**
     * @type {string[]}
     */
    set value(v) {
        this[key] = v;
    }
}
"#,
    );

    assert!(
        output.contains("set value(v: string[]);"),
        "Expected nested `(null | string)[]` backing type not to inject top-level null into setter type: {output}"
    );
    assert!(
        !output.contains("set value(v: string[] | null);"),
        "Did not expect nested element union nullability to be appended at top level: {output}"
    );
}

#[test]
fn test_property_access_to_unannotated_getter_uses_paired_setter_type() {
    let output = emit_dts_with_usage_analysis(
        r#"
class C {
    value: number;
    method(input: number) {
        return this.value + input;
    }
    get prop() {
        return this.method(this.value);
    }
    set prop(value: number) {
        this.value = this.method(value);
    }
}
const c = new C();
const propValue = c.prop;
"#,
    );

    assert!(
        output.contains("declare const propValue: number;"),
        "Expected property access to recover the paired setter type: {output}"
    );
}
