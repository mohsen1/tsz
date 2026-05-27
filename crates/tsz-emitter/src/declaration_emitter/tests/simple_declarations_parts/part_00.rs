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
fn namespace_overload_names_do_not_hide_outer_function_implementation() {
    let source = r#"
declare namespace ns {
    function f(): C;
    class C {}
}

function f() {
    return ns.f();
}
"#;
    let output = emit_dts_with_usage_analysis(source);

    assert!(
        output.contains("declare function f(): C;"),
        "Expected outer function declaration despite namespace overload: {output}"
    );
}

#[test]
fn non_ambient_namespace_without_exported_surface_emits_empty_namespace() {
    let source = r#"
namespace hidden {
    class C {
        private value;
    }
    interface I {
        [n: number]: C;
    }
}
"#;
    let output = emit_dts_with_usage_analysis(source);

    assert!(
        output.contains("declare namespace hidden {\n}"),
        "Expected hidden namespace to emit without private members: {output}"
    );
    assert!(
        !output.contains("class C"),
        "Expected hidden class to stay private: {output}"
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
fn test_internal_namespace_does_not_retain_private_module_interface() {
    let source = r#"
export var x = 1;
interface Iterator<T> {
    value: T;
}

namespace Query {
    export function fromDoWhile<T>(doWhile: (test: Iterator<T>) => boolean): Iterator<T> {
        return null;
    }
}
"#;
    let output = emit_dts_with_usage_analysis(source);

    assert_eq!(
        "export declare var x: number;",
        output.trim(),
        "Expected private namespace dependencies to stay out of module DTS: {output}"
    );
}

#[test]
fn test_exported_namespace_retains_private_module_interface_dependency() {
    let source = r#"
export var x = 1;
interface Iterator<T> {
    value: T;
}

export namespace Query {
    export function fromDoWhile<T>(doWhile: (test: Iterator<T>) => boolean): Iterator<T> {
        return null;
    }
}
"#;
    let output = emit_dts_with_usage_analysis(source);

    assert!(
        output.contains("interface Iterator<T>"),
        "Expected private interface to remain when exported namespace references it: {output}"
    );
    assert!(
        output.contains("export declare namespace Query"),
        "Expected exported namespace to remain public: {output}"
    );
    assert!(
        output.contains("test: Iterator<T>"),
        "Expected exported namespace member to keep its private dependency reference: {output}"
    );
}

#[test]
fn test_named_exported_namespace_retains_private_module_interface_dependency() {
    let source = r#"
export { Query as PublicQuery };
interface Private {
    value: string;
}

namespace Query {
    export function f(x: Private): Private {
        return x;
    }
}
"#;
    let output = emit_dts_with_usage_analysis(source);

    assert!(
        output.contains("export { Query as PublicQuery };"),
        "Expected aliased named namespace export to remain public: {output}"
    );
    assert!(
        output.contains("interface Private"),
        "Expected private interface to remain when named-exported namespace references it: {output}"
    );
    assert!(
        output.contains("function f(x: Private): Private;"),
        "Expected named-exported namespace member to keep its private dependency reference: {output}"
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
fn test_js_local_renamed_export_function_with_jsdoc_emits_before_alias_group() {
    let source = r#"
export function i() {}
/**
 * @param {number} x
 */
function hh(x) {
    return x;
}
export { hh as h };
export function j() {}
"#;
    let output = emit_js_dts_with_usage_analysis(source);

    let i_pos = output
        .find("export function i(): void;")
        .unwrap_or_else(|| panic!("missing i declaration: {output}"));
    let j_pos = output
        .find("export function j(): void;")
        .unwrap_or_else(|| panic!("missing j declaration: {output}"));
    let hh_pos = output
        .find("declare function hh(x: number): number;")
        .unwrap_or_else(|| panic!("missing deferred hh declaration: {output}"));
    let alias_pos = output
        .find("export { hh as h };")
        .unwrap_or_else(|| panic!("missing alias group: {output}"));

    assert!(
        i_pos < j_pos && j_pos < hh_pos && hh_pos < alias_pos,
        "Expected JSDoc-typed local export alias function to emit before the trailing alias group: {output}"
    );
    assert!(
        output.contains("/**\n * @param {number} x\n */\ndeclare function hh"),
        "Expected deferred local function to keep its JSDoc comment: {output}"
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
fn test_js_cjs_synthetic_function_export_with_jsdoc_is_not_alias_deferred() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @param {number} value
 */
module.exports.map = function map(value) {
    return value;
}
"#,
    );

    assert!(
        output.contains("export function map(value: number): number;"),
        "Expected CJS synthetic function export to emit at its own statement: {output}"
    );
    assert!(
        !output.contains("@param"),
        "Expected signature JSDoc on direct CJS function exports to be consumed, not re-emitted: {output}"
    );
}

#[test]
fn test_jsdoc_same_file_typeof_commonjs_function_export_expands_static_surface() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
module.exports.make = function make() {}
module.exports.make.label = "ok";

/**
 * @param {{value: typeof module.exports.make}} input
 */
function use(input) {
    input.value();
}
module.exports.use = use;
"#,
    );

    assert!(
        output.contains("value: {\n        (): void;\n        label: string;\n    };"),
        "Expected same-file typeof module.exports function references to expand callable static surface: {output}"
    );
    assert!(
        output.contains("export function use(input: {"),
        "Expected local CJS function alias to keep the exported function surface: {output}"
    );
}

#[test]
fn test_js_commonjs_class_expando_declarations_follow_direct_named_exports() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
module.exports.foo = function foo() {}
module.exports.foo.Widget = class {}
module.exports.bar = function bar() {}
/**
 * @param {number} value
 */
function later(value) {
    return value;
}
module.exports.later = later;
"#,
    );

    let namespace_pos = output
        .find("export namespace foo")
        .unwrap_or_else(|| panic!("expected foo namespace in output: {output}"));
    let bar_pos = output
        .find("export function bar")
        .unwrap_or_else(|| panic!("expected direct bar export in output: {output}"));
    let class_pos = output
        .find("declare class Widget")
        .unwrap_or_else(|| panic!("expected Widget class declaration in output: {output}"));
    let later_pos = output
        .find("export function later")
        .unwrap_or_else(|| panic!("expected deferred later export in output: {output}"));

    assert!(
        namespace_pos < bar_pos && bar_pos < class_pos && class_pos < later_pos,
        "Expected class expando declarations after direct named exports and before deferred CJS aliases: {output}"
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
fn test_returned_object_literal_local_function_declarations_inline() {
    let output = emit_dts_with_usage_analysis(
        r#"
function foo<T>(v: T) {
    function a<T>(a: T) { return a; }
    function b(): T { return v; }

    function c<T>(v: T) {
        function a<T>(a: T) { return a; }
        function b(): T { return v; }
        return { a, b };
    }

    return { a, b, c };
}
"#,
    );

    let expected = r#"declare function foo<T>(v: T): {
    a: <T_1>(a: T_1) => T_1;
    b: () => T;
    c: <T_1>(v: T_1) => {
        a: <T_2>(a: T_2) => T_2;
        b: () => T_1;
    };
};"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected returned local function declarations to inline as object member function types: {output}"
    );
}

#[test]
fn test_returned_object_literal_local_function_overloads_preserve_signatures() {
    let output = emit_dts_with_usage_analysis(
        r#"
function foo() {
    function a(x: string): string;
    function a(x: number): number;
    function a(x: string | number) { return x; }

    return { a };
}
"#,
    );

    let expected = r#"declare function foo(): {
    a: {
        (x: string): string;
        (x: number): number;
    };
};"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected returned overloaded local function declarations to preserve every overload signature: {output}"
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

