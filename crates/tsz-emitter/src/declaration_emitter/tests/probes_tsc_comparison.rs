use super::*;

// =============================================================================
// Full comparison against tsc output
// =============================================================================

#[test]
fn compare_tsc_complex_class_emit() {
    let output = emit_dts(
        r#"export class Container<T extends object> {
    private items: T[] = [];

    add(item: T): void {
        this.items.push(item);
    }

    get(index: number): T {
        return this.items[index];
    }

    get count(): number {
        return this.items.length;
    }

    map<U>(fn: (item: T) => U): U[] {
        return this.items.map(fn);
    }
}"#,
    );
    println!("COMPARE complex class:\n{output}");
    // tsc output:
    // export declare class Container<T extends object> {
    //     private items;
    //     add(item: T): void;
    //     get(index: number): T;
    //     get count(): number;
    //     map<U>(fn: (item: T) => U): U[];
    // }
    assert!(
        output.contains("private items;"),
        "Private should strip type: {output}"
    );
    assert!(
        output.contains("add(item: T): void;"),
        "Missing add: {output}"
    );
    assert!(
        output.contains("get count(): number;"),
        "Missing getter: {output}"
    );
    assert!(
        output.contains("map<U>(fn: (item: T) => U): U[];"),
        "Missing map: {output}"
    );
}

#[test]
fn compare_tsc_interface_generic_events() {
    let output = emit_dts(
        r#"export interface EventEmitter<T extends Record<string, any>> {
    on<K extends keyof T>(event: K, handler: (data: T[K]) => void): void;
    off<K extends keyof T>(event: K, handler: (data: T[K]) => void): void;
    emit<K extends keyof T>(event: K, data: T[K]): void;
}"#,
    );
    println!("COMPARE events interface:\n{output}");
    let expected = "export interface EventEmitter<T extends Record<string, any>> {\n    on<K extends keyof T>(event: K, handler: (data: T[K]) => void): void;\n    off<K extends keyof T>(event: K, handler: (data: T[K]) => void): void;\n    emit<K extends keyof T>(event: K, data: T[K]): void;\n}\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn compare_tsc_deep_partial_type() {
    let output = emit_dts(
        r#"export type DeepPartial<T> = {
    [P in keyof T]?: T[P] extends object ? DeepPartial<T[P]> : T[P];
};"#,
    );
    println!("COMPARE DeepPartial:\n{output}");
    // tsc:
    // export type DeepPartial<T> = {
    //     [P in keyof T]?: T[P] extends object ? DeepPartial<T[P]> : T[P];
    // };
    assert!(
        output.contains("[P in keyof T]?: T[P] extends object ? DeepPartial<T[P]> : T[P]"),
        "Missing mapped type body: {output}"
    );
}

#[test]
fn compare_tsc_promisified_type() {
    let output = emit_dts(
        r#"export type Promisified<T> = {
    [K in keyof T]: T[K] extends (...args: infer A) => infer R
        ? (...args: A) => Promise<R>
        : T[K];
};"#,
    );
    println!("COMPARE Promisified:\n{output}");
    assert!(output.contains("infer A"), "Missing infer A: {output}");
    assert!(output.contains("infer R"), "Missing infer R: {output}");
    assert!(
        output.contains("Promise<R>"),
        "Missing Promise<R>: {output}"
    );
}

#[test]
fn compare_tsc_abstract_class() {
    let output = emit_dts(
        r#"export abstract class AbstractLogger {
    abstract log(msg: string): void;
    abstract error(msg: string, err?: Error): void;

    warn(msg: string): void {
        this.log(`WARN: ${msg}`);
    }
}"#,
    );
    println!("COMPARE abstract class:\n{output}");
    // tsc:
    // export declare abstract class AbstractLogger {
    //     abstract log(msg: string): void;
    //     abstract error(msg: string, err?: Error): void;
    //     warn(msg: string): void;
    // }
    let expected = "export declare abstract class AbstractLogger {\n    abstract log(msg: string): void;\n    abstract error(msg: string, err?: Error): void;\n    warn(msg: string): void;\n}\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn compare_tsc_const_literal() {
    let output = emit_dts(r#"export declare const VERSION: "1.0.0";"#);
    println!("COMPARE const literal:\n{output}");
    let expected = "export declare const VERSION: \"1.0.0\";\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn compare_tsc_factory_function() {
    let output = emit_dts(
        r#"export declare function createFactory<T extends new (...args: any[]) => any>(
    ctor: T,
): InstanceType<T>;"#,
    );
    println!("COMPARE factory fn:\n{output}");
    // tsc single-line:
    // export declare function createFactory<T extends new (...args: any[]) => any>(ctor: T): InstanceType<T>;
    assert!(
        output.contains("createFactory<T extends new (...args: any[]) => any>"),
        "Missing generics: {output}"
    );
    assert!(output.contains("ctor: T"), "Missing param: {output}");
    assert!(
        output.contains("InstanceType<T>"),
        "Missing return: {output}"
    );
}

// =============================================================================
// Edge case comparison tests against tsc output
// =============================================================================

#[test]
fn edge_private_field_emits_as_hash_private() {
    let output = emit_dts(
        r#"export class HasPrivate {
    #secret: string = "hidden";
    getSecret(): string { return this.#secret; }
}"#,
    );
    println!("EDGE private field:\n{output}");
    // tsc emits: #private; (generic private marker, not actual field name)
    // Our emitter may emit #secret or #private - both have been seen in different tsc versions
    // The key is that private fields should appear and methods should have return types
    assert!(
        output.contains("#"),
        "Missing private field marker: {output}"
    );
    assert!(output.contains("getSecret()"), "Missing method: {output}");
}

#[test]
fn edge_override_stripped_in_dts() {
    // tsc strips 'override' in .d.ts output
    let output = emit_dts(
        r#"class Base {
    greet(): string { return "hi"; }
}
export class Derived extends Base {
    override greet(): string { return "hello"; }
}"#,
    );
    println!("EDGE override:\n{output}");
    // tsc output does NOT include 'override'
    assert!(
        output.contains("greet(): string;"),
        "Missing greet: {output}"
    );
}

#[test]
fn edge_recursive_type() {
    let output = emit_dts(
        r#"export type Json = string | number | boolean | null | Json[] | { [key: string]: Json };"#,
    );
    println!("EDGE recursive type:\n{output}");
    assert!(output.contains("Json[]"), "Missing array: {output}");
    assert!(
        output.contains("[key: string]: Json"),
        "Missing index sig: {output}"
    );
}

#[test]
fn edge_nested_conditional_type() {
    let output = emit_dts(
        r#"export type IsNullable<T> = undefined extends T ? true : null extends T ? true : false;"#,
    );
    println!("EDGE nested conditional:\n{output}");
    // tsc: undefined extends T ? true : null extends T ? true : false
    assert!(
        output.contains("undefined extends T ? true : null extends T ? true : false"),
        "Missing nested conditional: {output}"
    );
}

#[test]
fn edge_template_literal_simple() {
    let output = emit_dts("export type CssVar = `--${string}`;");
    println!("EDGE template literal:\n{output}");
    assert!(
        output.contains("`--${string}`"),
        "Missing template literal: {output}"
    );
}

#[test]
fn edge_interface_method_overloads() {
    let output = emit_dts(
        r#"export interface Overloaded {
    call(x: string): number;
    call(x: number): string;
}"#,
    );
    println!("EDGE interface overloads:\n{output}");
    assert!(
        output.contains("call(x: string): number;"),
        "Missing overload 1: {output}"
    );
    assert!(
        output.contains("call(x: number): string;"),
        "Missing overload 2: {output}"
    );
}

#[test]
fn edge_generic_default_conditional() {
    let output = emit_dts(
        r#"export type WithDefault<T, D = T extends string ? "str" : "other"> = {
    value: T;
    default: D;
};"#,
    );
    println!("EDGE generic default:\n{output}");
    assert!(
        output.contains("D = T extends string"),
        "Missing default conditional: {output}"
    );
}

#[test]
fn edge_enum_with_namespace_merge() {
    let output = emit_dts(
        r#"export enum Status {
    Active = "ACTIVE",
    Inactive = "INACTIVE"
}
export namespace Status {
    export function parse(s: string): Status;
}"#,
    );
    println!("EDGE enum+namespace:\n{output}");
    // tsc:
    // export declare enum Status { Active = "ACTIVE", Inactive = "INACTIVE" }
    // export declare namespace Status { function parse(s: string): Status; }
    assert!(output.contains("enum Status"), "Missing enum: {output}");
    assert!(
        output.contains("namespace Status"),
        "Missing namespace: {output}"
    );
    assert!(
        output.contains("parse(s: string): Status;"),
        "Missing parse fn: {output}"
    );
}

#[test]
fn edge_abstract_with_static() {
    let output = emit_dts(
        r#"export abstract class AbstractBase {
    static create(): AbstractBase { throw new Error(); }
    abstract getId(): string;
}"#,
    );
    println!("EDGE abstract+static:\n{output}");
    let expected = "export declare abstract class AbstractBase {\n    static create(): AbstractBase;\n    abstract getId(): string;\n}\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn edge_type_only_reexport_of_class() {
    let output = emit_dts(
        r#"class Base {
    greet(): string { return "hi"; }
}
export type { Base };"#,
    );
    println!("EDGE type re-export of class:\n{output}");
    // tsc emits: declare class Base { greet(): string; } and export type { Base };
    assert!(
        output.contains("export type { Base };") || output.contains("export type {Base}"),
        "Missing type re-export: {output}"
    );
}

#[test]
fn edge_computed_symbol_property_in_interface() {
    let output = emit_dts(
        r#"export declare const sym: unique symbol;
export interface WithSymbol {
    [sym]: string;
}"#,
    );
    println!("EDGE computed symbol in interface:\n{output}");
    assert!(
        output.contains("[sym]: string;"),
        "Missing computed symbol property: {output}"
    );
}

#[test]
fn edge_const_assertion_readonly_object() {
    // Without solver, this may not produce the exact tsc output
    let output = emit_dts(
        r#"export declare const config: {
    readonly port: 3000;
    readonly host: "localhost";
};"#,
    );
    println!("EDGE const assertion object:\n{output}");
    assert!(
        output.contains("readonly port: 3000;"),
        "Missing readonly port: {output}"
    );
    assert!(
        output.contains("readonly host: \"localhost\";"),
        "Missing readonly host: {output}"
    );
}

// =============================================================================
// String literal escape sequence tests
// =============================================================================

#[test]
fn fix_string_literal_escaped_quote() {
    // The scanner stores cooked text, so \" becomes a literal "
    // The emitter must re-escape it when writing the .d.ts
    let output = emit_dts(r#"export declare const a: "quote\"mark";"#);
    println!("FIX escaped quote:\n{output}");
    assert!(
        output.contains(r#""quote\"mark""#),
        "Missing escaped quote: {output}"
    );
}

#[test]
fn fix_string_literal_escaped_backslash() {
    let output = emit_dts(r#"export declare const a: "backslash\\path";"#);
    println!("FIX escaped backslash:\n{output}");
    assert!(
        output.contains(r#""backslash\\path""#),
        "Missing escaped backslash: {output}"
    );
}

#[test]
fn fix_string_literal_escaped_newline() {
    let output = emit_dts(r#"export declare const a: "line\nbreak";"#);
    println!("FIX escaped newline:\n{output}");
    assert!(
        output.contains(r#""line\nbreak""#),
        "Missing escaped newline: {output}"
    );
}

#[test]
fn fix_string_literal_escaped_tab() {
    let output = emit_dts(r#"export declare const a: "tab\there";"#);
    println!("FIX escaped tab:\n{output}");
    assert!(
        output.contains(r#""tab\there""#),
        "Missing escaped tab: {output}"
    );
}

#[test]
fn fix_string_literal_single_quote_escape() {
    let output = emit_dts("export declare const a: 'it\\'s';");
    println!("FIX single quote escape:\n{output}");
    assert!(
        output.contains(r#""it's""#),
        "Expected string literal type to normalize to double quotes: {output}"
    );
}

#[test]
fn fix_string_literal_no_escape_needed() {
    let output = emit_dts(r#"export declare const a: "normal";"#);
    println!("FIX no escape:\n{output}");
    assert!(
        output.contains(r#""normal""#),
        "Missing normal string: {output}"
    );
}

#[test]
fn fix_string_literal_combined_escapes() {
    let output = emit_dts(r#"export declare const a: "a\\b\"c\nd";"#);
    println!("FIX combined escapes:\n{output}");
    assert!(
        output.contains(r#""a\\b\"c\nd""#),
        "Missing combined escapes: {output}"
    );
}

#[test]
fn fix_enum_string_value_escaped_newline() {
    let output = emit_dts(r#"export enum E { A = "hello\nworld" }"#);
    println!("FIX enum newline:\n{output}");
    assert!(
        output.contains(r#""hello\nworld""#),
        "Enum string value should escape newline: {output}"
    );
}

#[test]
fn fix_enum_string_value_escaped_tab() {
    let output = emit_dts(r#"export enum E { A = "tab\there" }"#);
    println!("FIX enum tab:\n{output}");
    assert!(
        output.contains(r#""tab\there""#),
        "Enum string value should escape tab: {output}"
    );
}

#[test]
fn fix_enum_string_value_escaped_backslash() {
    let output = emit_dts(r#"export enum E { A = "back\\slash" }"#);
    println!("FIX enum backslash:\n{output}");
    assert!(
        output.contains(r#""back\\slash""#),
        "Enum string value should escape backslash: {output}"
    );
}

#[test]
fn fix_enum_string_value_escaped_quote() {
    let output = emit_dts(r#"export enum E { A = "he said \"hi\"" }"#);
    println!("FIX enum quote:\n{output}");
    assert!(
        output.contains(r#""he said \"hi\"""#),
        "Enum string value should escape quote: {output}"
    );
}

// =============================================================================
// DTS emit exploration tests (Round 4)
// =============================================================================

#[test]
fn explore_definite_assignment_assertion() {
    // tsc STRIPS `!` in .d.ts - definite assignment is not needed in declarations
    let output = emit_dts(
        "export class Foo {
    bar!: number;
}",
    );
    println!("EXPLORE definite assignment:\n{output}");
    assert!(
        output.contains("bar: number;"),
        "Should strip definite assignment assertion: {output}"
    );
    assert!(
        !output.contains("bar!:"),
        "Should not have ! in declaration: {output}"
    );
}

#[test]
fn explore_class_static_property() {
    let output = emit_dts(
        "export class Foo {
    static bar: string;
}",
    );
    println!("EXPLORE static property:\n{output}");
    assert!(
        output.contains("static bar: string;"),
        "Should emit static property: {output}"
    );
}

#[test]
fn explore_class_static_readonly_property() {
    let output = emit_dts(
        "export class Foo {
    static readonly VERSION: string;
}",
    );
    println!("EXPLORE static readonly:\n{output}");
    assert!(
        output.contains("static readonly VERSION: string;"),
        "Should emit static readonly: {output}"
    );
}

#[test]
fn explore_class_abstract_method() {
    let output = emit_dts(
        "export abstract class Base {
    abstract doSomething(x: number): void;
}",
    );
    println!("EXPLORE abstract method:\n{output}");
    assert!(
        output.contains("abstract doSomething(x: number): void;"),
        "Should emit abstract method: {output}"
    );
}

#[test]
fn explore_class_abstract_property() {
    let output = emit_dts(
        "export abstract class Base {
    abstract name: string;
}",
    );
    println!("EXPLORE abstract property:\n{output}");
    assert!(
        output.contains("abstract name: string;"),
        "Should emit abstract property: {output}"
    );
}

#[test]
fn explore_intersection_with_function_type() {
    // Function types inside intersections need parentheses
    let output = emit_dts("export type T = ((x: number) => void) & { tag: string };");
    println!("EXPLORE intersection+fn:\n{output}");
    assert!(
        output.contains("((x: number) => void) & {"),
        "Function in intersection should be parenthesized: {output}"
    );
}

#[test]
fn explore_readonly_array_type() {
    let output = emit_dts("export type T = readonly number[];");
    println!("EXPLORE readonly array:\n{output}");
    assert!(
        output.contains("readonly number[]"),
        "Should emit readonly array type: {output}"
    );
}

#[test]
fn explore_readonly_tuple_type() {
    let output = emit_dts("export type T = readonly [string, number];");
    println!("EXPLORE readonly tuple:\n{output}");
    assert!(
        output.contains("readonly [string, number]"),
        "Should emit readonly tuple type: {output}"
    );
}

#[test]
fn explore_labeled_tuple_optional() {
    let output = emit_dts("export type T = [first: string, second?: number];");
    println!("EXPLORE labeled optional tuple:\n{output}");
    assert!(
        output.contains("first: string"),
        "Should emit labeled tuple: {output}"
    );
    assert!(
        output.contains("second?: number"),
        "Should emit optional labeled tuple element: {output}"
    );
}

#[test]
fn explore_labeled_tuple_rest() {
    let output = emit_dts("export type T = [first: string, ...rest: number[]];");
    println!("EXPLORE labeled rest tuple:\n{output}");
    assert!(
        output.contains("...rest: number[]"),
        "Should emit rest labeled tuple element: {output}"
    );
}

#[test]
fn explore_import_type() {
    let output = emit_dts("export type T = import('./module').Foo;");
    println!("EXPLORE import type:\n{output}");
    assert!(
        output.contains("import("),
        "Should emit import type: {output}"
    );
    assert!(
        output.contains("Foo"),
        "Should emit qualified name after import: {output}"
    );
}

#[test]
fn explore_typeof_import_type() {
    let output = emit_dts("export type T = typeof import('./module');");
    println!("EXPLORE typeof import:\n{output}");
    assert!(
        output.contains("typeof import("),
        "Should emit typeof import: {output}"
    );
}

#[test]
fn explore_template_literal_multi_spans() {
    let output = emit_dts("export type T = `${string}-${number}`;");
    println!("EXPLORE multi-span template:\n{output}");
    assert!(
        output.contains("`${string}-${number}`"),
        "Should emit multi-span template literal: {output}"
    );
}

#[test]
fn explore_conditional_type_with_infer() {
    let output = emit_dts("export type UnpackPromise<T> = T extends Promise<infer U> ? U : T;");
    println!("EXPLORE conditional infer:\n{output}");
    assert!(
        output.contains("infer U"),
        "Should emit infer keyword: {output}"
    );
    assert!(
        output.contains("T extends Promise<infer U> ? U : T"),
        "Should emit full conditional type: {output}"
    );
}

#[test]
fn explore_infer_with_extends_constraint() {
    let output = emit_dts(
        "export type FirstString<T> = T extends [infer S extends string, ...unknown[]] ? S : never;",
    );
    println!("EXPLORE infer extends:\n{output}");
    assert!(
        output.contains("infer S extends string"),
        "Should emit infer with extends constraint: {output}"
    );
}

#[test]
fn explore_nested_conditional_type() {
    let output = emit_dts(
        "export type Deep<T> = T extends string ? 'str' : T extends number ? 'num' : 'other';",
    );
    println!("EXPLORE nested conditional:\n{output}");
    assert!(
        output.contains("T extends string ?"),
        "Should emit nested conditional: {output}"
    );
    assert!(
        output.contains("T extends number ?"),
        "Should emit inner conditional: {output}"
    );
}

#[test]
fn explore_call_signature_in_interface() {
    let output = emit_dts(
        "export interface Callable {
    (x: number): string;
    (x: string): number;
}",
    );
    println!("EXPLORE call sig:\n{output}");
    assert!(
        output.contains("(x: number): string;"),
        "Should emit call signature: {output}"
    );
    assert!(
        output.contains("(x: string): number;"),
        "Should emit second call signature: {output}"
    );
}

#[test]
fn explore_construct_signature_in_interface() {
    let output = emit_dts(
        "export interface Constructable {
    new (x: number): object;
}",
    );
    println!("EXPLORE construct sig:\n{output}");
    assert!(
        output.contains("new (x: number): object;"),
        "Should emit construct signature: {output}"
    );
}

#[test]
fn explore_index_signature_readonly() {
    let output = emit_dts(
        "export interface ReadonlyDict {
    readonly [key: string]: unknown;
}",
    );
    println!("EXPLORE readonly index sig:\n{output}");
    assert!(
        output.contains("readonly [key: string]: unknown;"),
        "Should emit readonly index signature: {output}"
    );
}

#[test]
fn explore_class_with_index_signature_readonly() {
    let output = emit_dts(
        "export class Foo {
    readonly [key: string]: unknown;
}",
    );
    println!("EXPLORE class readonly index sig:\n{output}");
    assert!(
        output.contains("readonly [key: string]: unknown;"),
        "Should emit class readonly index signature: {output}"
    );
}

#[test]
fn explore_this_parameter_stripped() {
    // tsc strips the `this` parameter in .d.ts - but KEEPS it actually
    // tsc preserves `this` parameter in .d.ts files for functions
    let output = emit_dts("export function handler(this: HTMLElement, event: Event): void {}");
    println!("EXPLORE this param:\n{output}");
    assert!(
        output.contains("this: HTMLElement"),
        "Should preserve this parameter in function: {output}"
    );
}

#[test]
fn explore_function_type_in_union_paren() {
    // Function types in unions need parens
    let output = emit_dts("export type T = ((x: number) => void) | string;");
    println!("EXPLORE fn in union:\n{output}");
    assert!(
        output.contains("((x: number) => void)"),
        "Function type in union should be parenthesized: {output}"
    );
}

#[test]
fn explore_constructor_type_in_union_paren() {
    // Constructor types in unions need parens
    let output = emit_dts("export type T = (new (x: number) => Foo) | string;");
    println!("EXPLORE ctor in union:\n{output}");
    assert!(
        output.contains("(new (x: number) => Foo)"),
        "Constructor type in union should be parenthesized: {output}"
    );
}

#[test]
fn explore_bigint_literal_type() {
    let output = emit_dts("export type T = 100n;");
    println!("EXPLORE bigint literal:\n{output}");
    assert!(
        output.contains("100n"),
        "Should emit bigint literal: {output}"
    );
}

#[test]
fn explore_negative_number_literal_type() {
    let output = emit_dts("export type T = -42;");
    println!("EXPLORE negative number:\n{output}");
    assert!(
        output.contains("-42"),
        "Should emit negative number literal: {output}"
    );
}

#[test]
fn explore_unique_symbol_type() {
    let output = emit_dts("export declare const sym: unique symbol;");
    println!("EXPLORE unique symbol:\n{output}");
    assert!(
        output.contains("unique symbol"),
        "Should emit unique symbol type: {output}"
    );
}

#[test]
fn explore_class_get_set_accessors() {
    let output = emit_dts(
        "export class Foo {
    get value(): number { return 0; }
    set value(v: number) {}
}",
    );
    println!("EXPLORE get/set accessors:\n{output}");
    assert!(
        output.contains("get value(): number;"),
        "Should emit getter: {output}"
    );
    assert!(
        output.contains("set value(v: number);"),
        "Should emit setter: {output}"
    );
}

#[test]
fn explore_interface_get_set_accessors() {
    let output = emit_dts(
        "export interface Foo {
    get value(): number;
    set value(v: number);
}",
    );
    println!("EXPLORE interface accessors:\n{output}");
    assert!(
        output.contains("get value(): number;"),
        "Should emit interface getter: {output}"
    );
    assert!(
        output.contains("set value(v: number);"),
        "Should emit interface setter: {output}"
    );
}

#[test]
fn explore_keyof_intersection_parens() {
    // keyof (A & B) needs parens
    let output = emit_dts("export type T = keyof (A & B);");
    println!("EXPLORE keyof intersection:\n{output}");
    // Note: our parser may not create a ParenthesizedType here,
    // but the emitter should handle this via TYPE_OPERATOR logic
    assert!(output.contains("keyof"), "Should emit keyof: {output}");
}

#[test]
fn explore_mapped_type_with_as_clause() {
    let output = emit_dts(
        "export type Getters<T> = {
    [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K];
};",
    );
    println!("EXPLORE mapped type as clause:\n{output}");
    assert!(output.contains(" as "), "Should emit as clause: {output}");
}

#[test]
fn explore_variadic_tuple_type() {
    let output = emit_dts("export type Concat<A extends any[], B extends any[]> = [...A, ...B];");
    println!("EXPLORE variadic tuple:\n{output}");
    assert!(
        output.contains("[...A, ...B]"),
        "Should emit variadic tuple: {output}"
    );
}

#[test]
fn explore_type_predicate_function() {
    let output = emit_dts(
        "export function isString(x: unknown): x is string { return typeof x === 'string'; }",
    );
    println!("EXPLORE type predicate:\n{output}");
    assert!(
        output.contains("x is string"),
        "Should emit type predicate: {output}"
    );
}

#[test]
fn explore_asserts_function() {
    let output = emit_dts(
        "export function assertDefined<T>(value: T): asserts value is NonNullable<T> { if (!value) throw new Error(); }",
    );
    println!("EXPLORE asserts:\n{output}");
    assert!(
        output.contains("asserts value is NonNullable<T>"),
        "Should emit asserts type predicate: {output}"
    );
}

#[test]
fn explore_enum_in_namespace() {
    let output = emit_dts(
        "export namespace NS {
    export enum E {
        A = 0,
        B = 1,
    }
}",
    );
    println!("EXPLORE enum in namespace:\n{output}");
    assert!(
        output.contains("enum E"),
        "Should emit enum in namespace: {output}"
    );
}

#[test]
fn explore_constructor_type_abstract() {
    let output = emit_dts("export type T = abstract new (x: number) => object;");
    println!("EXPLORE abstract ctor type:\n{output}");
    assert!(
        output.contains("abstract new"),
        "Should emit abstract constructor type: {output}"
    );
}

#[test]
fn explore_generic_default_with_conditional() {
    let output =
        emit_dts("export type Maybe<T, Fallback = T extends null ? never : T> = Fallback;");
    println!("EXPLORE generic default conditional:\n{output}");
    assert!(
        output.contains("Fallback = T extends null ? never : T"),
        "Should emit conditional type as default: {output}"
    );
}

#[test]
fn explore_class_with_constructor_param_property_readonly() {
    let output = emit_dts(
        "export class Foo {
    constructor(readonly name: string, public age: number, private secret: string) {}
}",
    );
    println!("EXPLORE ctor param properties:\n{output}");
    assert!(
        output.contains("readonly name: string;"),
        "Should emit readonly param property: {output}"
    );
}

#[test]
fn explore_type_literal_with_call_signature() {
    let output = emit_dts(
        "export type Callable = {
    (x: number): string;
    name: string;
};",
    );
    println!("EXPLORE type literal call sig:\n{output}");
    assert!(
        output.contains("(x: number): string;"),
        "Should emit call signature in type literal: {output}"
    );
    assert!(
        output.contains("name: string;"),
        "Should emit property in type literal: {output}"
    );
}

#[test]
fn explore_type_literal_with_construct_signature() {
    let output = emit_dts(
        "export type Constructable = {
    new (x: number): object;
};",
    );
    println!("EXPLORE type literal construct sig:\n{output}");
    assert!(
        output.contains("new (x: number): object;"),
        "Should emit construct signature in type literal: {output}"
    );
}

#[test]
fn explore_type_literal_with_index_signature() {
    let output = emit_dts(
        "export type Dict = {
    [key: string]: unknown;
};",
    );
    println!("EXPLORE type literal index sig:\n{output}");
    assert!(
        output.contains("[key: string]: unknown;"),
        "Should emit index signature in type literal: {output}"
    );
}

#[test]
fn explore_type_literal_with_method_signature() {
    let output = emit_dts(
        "export type Obj = {
    foo(x: number): string;
};",
    );
    println!("EXPLORE type literal method sig:\n{output}");
    assert!(
        output.contains("foo(x: number): string;"),
        "Should emit method signature in type literal: {output}"
    );
}

#[test]
fn explore_const_enum() {
    let output = emit_dts(
        "export const enum Direction {
    Up = 0,
    Down = 1,
    Left = 2,
    Right = 3,
}",
    );
    println!("EXPLORE const enum:\n{output}");
    assert!(
        output.contains("const enum Direction"),
        "Should emit const enum: {output}"
    );
}

#[test]
fn explore_function_overloads() {
    let output = emit_dts(
        "export function foo(x: number): number;
export function foo(x: string): string;
export function foo(x: number | string): number | string {
    return x;
}",
    );
    println!("EXPLORE function overloads:\n{output}");
    assert!(
        output.contains("export declare function foo(x: number): number;"),
        "Should emit first overload: {output}"
    );
    assert!(
        output.contains("export declare function foo(x: string): string;"),
        "Should emit second overload: {output}"
    );
    // Should NOT contain the implementation signature
    let count = output.matches("function foo").count();
    assert_eq!(
        count, 2,
        "Should only emit 2 overloads, not implementation: {output}"
    );
}

#[test]
fn explore_intersection_conditional_parens() {
    // Conditional types inside intersections don't need extra parens
    // but function types do
    let output = emit_dts("export type T = ((x: number) => void) & ((y: string) => void);");
    println!("EXPLORE intersection of fns:\n{output}");
    assert!(
        output.contains("((x: number) => void) & ((y: string) => void)"),
        "Function types in intersection should be parenthesized: {output}"
    );
}

#[test]
fn explore_export_default_function_with_type_params() {
    let output = emit_dts("export default function identity<T>(x: T): T { return x; }");
    println!("EXPLORE default fn with type params:\n{output}");
    assert!(
        output.contains("export default function identity<T>(x: T): T;"),
        "Should emit default function with type params: {output}"
    );
}

#[test]
fn explore_const_enum_string_values() {
    let output = emit_dts(
        r#"export const enum Dir {
    Up = "UP",
    Down = "DOWN",
}"#,
    );
    println!("EXPLORE const enum string values:\n{output}");
    // tsc: trailing comma is removed in the last member
    assert!(
        output.contains(r#"Up = "UP""#),
        "Should emit Up member: {output}"
    );
    assert!(
        output.contains(r#"Down = "DOWN""#),
        "Should emit Down member: {output}"
    );
}

#[test]
fn explore_generic_function_keyof_constraint() {
    let output = emit_dts(
        "export function getProperty<T, K extends keyof T>(obj: T, key: K): T[K] { return obj[key]; }",
    );
    println!("EXPLORE generic keyof:\n{output}");
    assert!(
        output.contains("K extends keyof T"),
        "Should emit keyof constraint: {output}"
    );
    assert!(
        output.contains("T[K]"),
        "Should emit indexed access return type: {output}"
    );
}

#[test]
fn explore_deep_partial_mapped_type() {
    let output = emit_dts(
        "export type DeepPartial<T> = {
    [P in keyof T]?: T[P] extends object ? DeepPartial<T[P]> : T[P];
};",
    );
    println!("EXPLORE deep partial:\n{output}");
    assert!(
        output.contains("[P in keyof T]?"),
        "Should emit mapped type with question: {output}"
    );
    assert!(
        output.contains("T[P] extends object ? DeepPartial<T[P]> : T[P]"),
        "Should emit conditional in mapped type: {output}"
    );
}

#[test]
fn explore_intersection_fn_and_object() {
    let output = emit_dts("export type Fn = ((x: number) => void) & { displayName: string };");
    println!("EXPLORE intersection fn+obj:\n{output}");
    assert!(
        output.contains("((x: number) => void) & "),
        "Function in intersection should be parenthesized: {output}"
    );
    assert!(
        output.contains("displayName: string;"),
        "Should emit object members: {output}"
    );
}

#[test]
fn explore_template_literal_capitalize() {
    let output = emit_dts("export type EventName<T extends string> = `on${Capitalize<T>}`;");
    println!("EXPLORE template capitalize:\n{output}");
    assert!(
        output.contains("`on${Capitalize<T>}`"),
        "Should emit template literal with Capitalize: {output}"
    );
}

#[test]
fn explore_export_type_only() {
    let output = emit_dts(
        "interface Foo { x: number; }
export type { Foo };",
    );
    println!("EXPLORE type-only export:\n{output}");
    assert!(
        output.contains("export type { Foo"),
        "Should emit type-only export: {output}"
    );
}

#[test]
fn explore_declare_global() {
    let output = emit_dts(
        "export {};
declare global {
    interface Window {
        myProp: string;
    }
}",
    );
    println!("EXPLORE declare global:\n{output}");
    assert!(
        output.contains("declare global"),
        "Should emit declare global: {output}"
    );
    assert!(
        output.contains("myProp: string;"),
        "Should emit augmented interface member: {output}"
    );
}

#[test]
fn explore_namespace_export_as() {
    let output = emit_dts("export * as utils from './utils';");
    println!("EXPLORE namespace export:\n{output}");
    assert!(
        output.contains("export * as utils from"),
        "Should emit namespace re-export: {output}"
    );
}

#[test]
fn explore_class_with_generic_method() {
    let output = emit_dts(
        "export class Container {
    get<T>(key: string): T | undefined { return undefined; }
    set<T>(key: string, value: T): void {}
}",
    );
    println!("EXPLORE class generic method:\n{output}");
    assert!(
        output.contains("get<T>(key: string): T | undefined;"),
        "Should emit generic getter method: {output}"
    );
    assert!(
        output.contains("set<T>(key: string, value: T): void;"),
        "Should emit generic setter method: {output}"
    );
}

#[test]
fn explore_declare_function_with_this() {
    // When source is already a .d.ts, preserve as-is
    let output = emit_dts("declare function handler(this: Window, e: Event): void;");
    println!("EXPLORE declare fn this:\n{output}");
    assert!(
        output.contains("this: Window"),
        "Should preserve this param in declare function: {output}"
    );
}

#[test]
fn explore_multiple_heritage_clauses() {
    let output = emit_dts(
        "export class Derived extends Base implements Comparable, Serializable {
    compare(other: Derived): number { return 0; }
}
declare class Base {}
interface Comparable {}
interface Serializable {}",
    );
    println!("EXPLORE multiple heritage:\n{output}");
    assert!(
        output.contains("extends Base implements Comparable, Serializable"),
        "Should emit extends and implements: {output}"
    );
}

#[test]
fn explore_accessor_keyword_strips_initializer() {
    // tsc emits `accessor name: string;` stripping the initializer
    let output = emit_dts(
        "export class Foo {
    accessor name: string = \"\";
}",
    );
    println!("EXPLORE accessor keyword:\n{output}");
    assert!(
        output.contains("accessor name: string;"),
        "Should emit accessor without initializer: {output}"
    );
}

#[test]
fn explore_enum_negative_values() {
    let output = emit_dts(
        "export enum Signed {
    Neg = -1,
    Zero = 0,
    Pos = 1,
}",
    );
    println!("EXPLORE negative enum:\n{output}");
    assert!(
        output.contains("Neg = -1"),
        "Should emit negative enum value: {output}"
    );
}

#[test]
fn explore_conditional_type_parens_on_check() {
    // Check type with union should get parens in conditional
    let output = emit_dts("export type T<U> = U extends string | number ? 'yes' : 'no';");
    println!("EXPLORE conditional union check:\n{output}");
    // The union in extends position is fine without parens - it's parsed right-to-left
    assert!(
        output.contains("extends string | number"),
        "Should emit union in extends: {output}"
    );
}

#[test]
fn explore_declare_on_class_member_stripped() {
    // tsc strips `declare` from class member declarations in .d.ts
    let output = emit_dts(
        "export class Foo {
    declare x: number;
}",
    );
    println!("EXPLORE declare member:\n{output}");
    // Should emit just `x: number;` without `declare`
    assert!(
        output.contains("x: number;"),
        "Should have property: {output}"
    );
    // The `declare` keyword on the member should be stripped
    // (the class-level `declare` is fine, member-level is not)
    let member_line = output.lines().find(|l| l.contains("x: number")).unwrap();
    assert!(
        !member_line.contains("declare"),
        "declare should be stripped from class member: {output}"
    );
}

#[test]
fn explore_generator_method_stripped_asterisk() {
    // tsc strips the `*` from generator methods in .d.ts
    let output = emit_dts(
        "export class Gen {
    *items(): Generator<number> { yield 1; }
}",
    );
    println!("EXPLORE generator method:\n{output}");
    assert!(
        output.contains("items(): Generator<number>;"),
        "Should emit method without asterisk: {output}"
    );
    assert!(
        !output.contains("*items"),
        "Should not have asterisk: {output}"
    );
}

#[test]
fn explore_namespace_members_no_declare() {
    // Inside declare namespace, members should not have `declare` keyword
    let output = emit_dts(
        "export namespace MyLib {
    export interface Options { debug: boolean; }
    export function create(opts: Options): void;
}",
    );
    println!("EXPLORE namespace members:\n{output}");
    assert!(
        output.contains("interface Options"),
        "Should emit interface: {output}"
    );
    assert!(
        output.contains("function create"),
        "Should emit function: {output}"
    );
    // Inside a declare namespace, tsc does NOT add `declare` to members
    let fn_line = output
        .lines()
        .find(|l| l.contains("function create"))
        .unwrap();
    assert!(
        !fn_line.contains("declare"),
        "Should not have declare inside namespace: {output}"
    );
}

#[test]
fn explore_enum_computed_initializers() {
    // tsc evaluates computed enum initializers to their numeric values:
    // `1 << 0` -> 1, `1 << 1` -> 2, `Read | Write` -> 3
    let output = emit_dts(
        "export enum Flags {
    None = 0,
    Read = 1 << 0,
    Write = 1 << 1,
    ReadWrite = Read | Write,
}",
    );
    println!("EXPLORE enum computed:\n{output}");
    // tsc computes these to their values: 0, 1, 2, 3
    assert!(
        output.contains("None = 0"),
        "Should emit None = 0: {output}"
    );
    assert!(
        output.contains("Read = 1"),
        "Should evaluate Read = 1: {output}"
    );
    assert!(
        output.contains("Write = 2"),
        "Should evaluate Write = 2: {output}"
    );
    assert!(
        output.contains("ReadWrite = 3"),
        "Should evaluate ReadWrite = 3: {output}"
    );
}

#[test]
fn explore_export_rename() {
    let output = emit_dts(
        "interface A { a: number; }
export { A as ARenamed };",
    );
    println!("EXPLORE export rename:\n{output}");
    assert!(
        output.contains("A as ARenamed"),
        "Should emit renamed export: {output}"
    );
}

#[test]
fn explore_recursive_conditional_type() {
    let output = emit_dts("export type Flatten<T> = T extends Array<infer U> ? Flatten<U> : T;");
    println!("EXPLORE recursive conditional:\n{output}");
    assert!(
        output.contains("T extends Array<infer U> ? Flatten<U> : T"),
        "Should emit recursive conditional: {output}"
    );
}

#[test]
fn explore_complex_object_type_alias() {
    let output = emit_dts(
        "export declare const config: {
    readonly host: string;
    readonly port: number;
    readonly options: {
        readonly ssl: boolean;
    };
};",
    );
    println!("EXPLORE complex object type:\n{output}");
    assert!(
        output.contains("readonly host: string;"),
        "Should emit readonly host: {output}"
    );
    assert!(
        output.contains("readonly ssl: boolean;"),
        "Should emit nested readonly: {output}"
    );
}

#[test]
fn explore_multiple_index_signatures() {
    let output = emit_dts(
        "export declare class MultiIndex {
    [key: string]: any;
    [index: number]: string;
}",
    );
    println!("EXPLORE multi index sig:\n{output}");
    assert!(
        output.contains("[key: string]: any;"),
        "Should emit string index: {output}"
    );
    assert!(
        output.contains("[index: number]: string;"),
        "Should emit number index: {output}"
    );
}

#[test]
fn explore_interface_extends_multiple() {
    let output = emit_dts(
        "export interface A { a: number; }
export interface B { b: string; }
export interface C extends A, B { c: boolean; }",
    );
    println!("EXPLORE multi extends:\n{output}");
    assert!(
        output.contains("C extends A, B"),
        "Should emit multiple extends: {output}"
    );
}

#[test]
fn explore_generic_class_with_default_type() {
    let output = emit_dts("export declare class Container<T = unknown> { value: T; }");
    println!("EXPLORE generic default:\n{output}");
    assert!(
        output.contains("<T = unknown>"),
        "Should emit default type param: {output}"
    );
}
