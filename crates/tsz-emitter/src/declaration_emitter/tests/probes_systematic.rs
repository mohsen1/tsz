use super::*;

// =============================================================================
// Systematic DTS emit probes
// =============================================================================

#[test]
fn probe_abstract_class_emit() {
    let output = emit_dts(
        "export abstract class Shape {
    abstract area(): number;
    abstract readonly name: string;
}",
    );
    println!("PROBE abstract class:\n{output}");
    assert!(
        output.contains("export declare abstract class Shape"),
        "Missing abstract: {output}"
    );
    assert!(
        output.contains("abstract area(): number;"),
        "Missing abstract method: {output}"
    );
    assert!(
        output.contains("abstract readonly name: string;"),
        "Missing abstract readonly: {output}"
    );
}

#[test]
fn probe_const_enum_emit() {
    let output = emit_dts("export const enum Direction { Up = \"UP\", Down = \"DOWN\" }");
    println!("PROBE const enum:\n{output}");
    assert!(
        output.contains("export declare const enum Direction"),
        "Missing const enum: {output}"
    );
    assert!(
        output.contains("Up = \"UP\""),
        "Missing enum member: {output}"
    );
}

#[test]
fn probe_function_overloads() {
    let output = emit_dts(
        r#"export function foo(x: string): number;
export function foo(x: number): string;
export function foo(x: any): any { return x; }"#,
    );
    println!("PROBE overloads:\n{output}");
    assert!(
        output.contains("export declare function foo(x: string): number;"),
        "Missing overload 1: {output}"
    );
    assert!(
        output.contains("export declare function foo(x: number): string;"),
        "Missing overload 2: {output}"
    );
    // Implementation should NOT be emitted
    assert!(
        !output.contains("x: any): any"),
        "Implementation leaked: {output}"
    );
}

#[test]
fn probe_default_export_function() {
    let output = emit_dts("export default function foo(): void {}");
    println!("PROBE default fn:\n{output}");
    assert!(
        output.contains("export default function foo(): void;"),
        "Missing default fn: {output}"
    );
}

#[test]
fn probe_template_literal_type() {
    let output = emit_dts("export type Ev = `${'click' | 'scroll'}_handler`;");
    println!("PROBE template literal:\n{output}");
    assert!(
        output.contains("`${'click' | 'scroll'}_handler`")
            || output.contains("`${\"click\" | \"scroll\"}_handler`"),
        "Missing template literal: {output}"
    );
}

#[test]
fn probe_mapped_type_as_clause() {
    let output =
        emit_dts("export type Getters<T> = { [K in keyof T as `get${string & K}`]: () => T[K] };");
    println!("PROBE mapped as:\n{output}");
    assert!(
        output.contains("as `get${string & K}`"),
        "Missing as clause: {output}"
    );
}

#[test]
fn probe_conditional_type() {
    let output = emit_dts("export type IsStr<T> = T extends string ? true : false;");
    println!("PROBE conditional:\n{output}");
    assert!(
        output.contains("T extends string ? true : false"),
        "Missing conditional: {output}"
    );
}

#[test]
fn probe_call_construct_signatures() {
    let output = emit_dts(
        "export interface Factory {
    (arg: string): object;
    new (arg: string): object;
}",
    );
    println!("PROBE call+construct:\n{output}");
    assert!(
        output.contains("(arg: string): object;"),
        "Missing call sig: {output}"
    );
    assert!(
        output.contains("new (arg: string): object;"),
        "Missing construct sig: {output}"
    );
}

#[test]
fn probe_named_tuple_members() {
    let output = emit_dts("export type Point = [x: number, y: number, z?: number];");
    println!("PROBE named tuple:\n{output}");
    assert!(
        output.contains("x: number"),
        "Missing named member x: {output}"
    );
    assert!(
        output.contains("z?: number"),
        "Missing optional named member z: {output}"
    );
}

#[test]
fn probe_import_type() {
    let output = emit_dts("export type T = import('./mod').Foo;");
    println!("PROBE import type:\n{output}");
    assert!(output.contains("import("), "Missing import type: {output}");
}

#[test]
fn probe_unique_symbol() {
    let output = emit_dts("export declare const sym: unique symbol;");
    println!("PROBE unique symbol:\n{output}");
    assert!(
        output.contains("unique symbol"),
        "Missing unique symbol: {output}"
    );
}

#[test]
fn probe_type_predicate() {
    let output = emit_dts("export function isString(x: unknown): x is string;");
    println!("PROBE type predicate:\n{output}");
    assert!(
        output.contains("x is string"),
        "Missing type predicate: {output}"
    );
}

#[test]
fn probe_assertion_function_with_type() {
    let output = emit_dts("export function assertStr(x: unknown): asserts x is string;");
    println!("PROBE assertion fn:\n{output}");
    assert!(
        output.contains("asserts x is string"),
        "Missing assertion: {output}"
    );
}

#[test]
fn probe_infer_type() {
    let output = emit_dts("export type Unwrap<T> = T extends Promise<infer U> ? U : T;");
    println!("PROBE infer:\n{output}");
    assert!(output.contains("infer U"), "Missing infer: {output}");
}

#[test]
fn probe_parameter_properties() {
    let output = emit_dts(
        "export class Foo {
    constructor(public readonly x: number, private y: string, protected z: boolean) {}
}",
    );
    println!("PROBE param props:\n{output}");
    assert!(
        output.contains("readonly x: number;"),
        "Missing readonly x: {output}"
    );
    // tsc strips type annotations from private members in .d.ts
    assert!(output.contains("private y;"), "Missing private y: {output}");
    assert!(
        output.contains("protected z: boolean;"),
        "Missing protected z: {output}"
    );
}

#[test]
fn probe_constructor_type() {
    let output = emit_dts("export type T = new (x: string) => object;");
    println!("PROBE constructor type:\n{output}");
    assert!(
        output.contains("new (x: string) => object"),
        "Missing constructor type: {output}"
    );
}

#[test]
fn probe_abstract_constructor_type() {
    let output = emit_dts("export type T = abstract new (x: string) => object;");
    println!("PROBE abstract constructor:\n{output}");
    assert!(
        output.contains("abstract new (x: string) => object"),
        "Missing abstract constructor: {output}"
    );
}

#[test]
fn probe_declare_module() {
    let output = emit_dts(
        "declare module 'my-module' {
    export function foo(): void;
    export const bar: string;
}",
    );
    println!("PROBE declare module:\n{output}");
    assert!(
        output.contains("declare module 'my-module'")
            || output.contains("declare module \"my-module\""),
        "Missing declare module: {output}"
    );
    assert!(
        output.contains("function foo(): void;"),
        "Missing fn in module: {output}"
    );
}

#[test]
fn probe_generic_class_with_constraint() {
    let output = emit_dts(
        "export class Container<T extends object> {
    value: T;
}",
    );
    println!("PROBE generic class:\n{output}");
    assert!(
        output.contains("T extends object"),
        "Missing constraint: {output}"
    );
    assert!(output.contains("value: T;"), "Missing value: {output}");
}

#[test]
fn probe_typeof_type() {
    let output = emit_dts(
        "export declare const x: number;
export type T = typeof x;",
    );
    println!("PROBE typeof:\n{output}");
    assert!(output.contains("typeof x"), "Missing typeof: {output}");
}

#[test]
fn probe_readonly_array_type() {
    let output = emit_dts("export type T = readonly string[];");
    println!("PROBE readonly array:\n{output}");
    assert!(
        output.contains("readonly string[]"),
        "Missing readonly array: {output}"
    );
}

#[test]
fn probe_indexed_access_type() {
    let output = emit_dts("export type T = string[][0];");
    println!("PROBE indexed access:\n{output}");
    assert!(
        output.contains("string[][0]"),
        "Missing indexed access: {output}"
    );
}

#[test]
fn probe_intersection_type() {
    let output = emit_dts("export type T = { a: string } & { b: number };");
    println!("PROBE intersection:\n{output}");
    assert!(output.contains("a: string"), "Missing a: {output}");
    assert!(output.contains("b: number"), "Missing b: {output}");
    assert!(output.contains("&"), "Missing intersection: {output}");
}

#[test]
fn probe_optional_tuple_element() {
    let output = emit_dts("export type T = [string?];");
    println!("PROBE optional tuple:\n{output}");
    assert!(
        output.contains("string?"),
        "Missing optional element: {output}"
    );
}

#[test]
fn probe_rest_tuple_element() {
    let output = emit_dts("export type T = [string, ...number[]];");
    println!("PROBE rest tuple:\n{output}");
    assert!(
        output.contains("...number[]"),
        "Missing rest element: {output}"
    );
}

#[test]
fn probe_bigint_literal_type() {
    let output = emit_dts("export type T = 42n;");
    println!("PROBE bigint:\n{output}");
    assert!(output.contains("42n"), "Missing bigint: {output}");
}

#[test]
fn probe_negative_literal_type() {
    let output = emit_dts("export type T = -1;");
    println!("PROBE negative literal:\n{output}");
    assert!(output.contains("-1"), "Missing negative: {output}");
}

#[test]
fn probe_interface_multiple_extends() {
    let output = emit_dts(
        "interface A { a: string; }
interface B { b: number; }
export interface C extends A, B { c: boolean; }",
    );
    println!("PROBE multi extends:\n{output}");
    assert!(
        output.contains("extends A, B"),
        "Missing multi extends: {output}"
    );
}

#[test]
fn probe_private_field() {
    let output = emit_dts(
        "export class Foo {
    #bar: string = '';
}",
    );
    println!("PROBE private field:\n{output}");
    // tsc emits `#bar: string;` or just omits it. Let's see.
    // Actually tsc keeps #bar in .d.ts
    println!("Private field output: {output}");
}

#[test]
fn probe_export_default_abstract_class() {
    let output = emit_dts("export default abstract class { abstract foo(): void; }");
    println!("PROBE default abstract:\n{output}");
    assert!(
        output.contains("export default abstract class"),
        "Missing default abstract: {output}"
    );
    assert!(
        output.contains("abstract foo(): void;"),
        "Missing abstract method: {output}"
    );
}

#[test]
fn probe_declare_keyword_passthrough() {
    let output = emit_dts(
        "export declare function foo(): void;
export declare class Bar {}
export declare const baz: number;
export declare enum E { A }",
    );
    println!("PROBE declare passthrough:\n{output}");
    assert!(
        output.contains("export declare function foo(): void;"),
        "Missing declare fn: {output}"
    );
    assert!(
        output.contains("export declare class Bar"),
        "Missing declare class: {output}"
    );
    assert!(
        output.contains("export declare const baz: number;"),
        "Missing declare const: {output}"
    );
    assert!(
        output.contains("export declare enum E"),
        "Missing declare enum: {output}"
    );
}

#[test]
fn probe_import_equals() {
    let output = emit_dts(
        "import Foo = require('./foo');
export = Foo;",
    );
    println!("PROBE import equals (no binding):\n{output}");
    // Without binding, import elision may drop the import.
    // With binding, it should be preserved.
    let output2 = emit_dts_with_usage_analysis(
        "import Foo = require('./foo');
export = Foo;",
    );
    println!("PROBE import equals (with binding):\n{output2}");
    assert!(
        output2.contains("import Foo"),
        "Missing import (with binding): {output2}"
    );
    assert!(
        output2.contains("export = Foo;"),
        "Missing export = (with binding): {output2}"
    );
}

#[test]
fn probe_keyof_type() {
    let output = emit_dts("export type Keys<T> = keyof T;");
    println!("PROBE keyof:\n{output}");
    assert!(output.contains("keyof T"), "Missing keyof: {output}");
}

#[test]
fn probe_class_implements() {
    let output = emit_dts(
        "interface Printable { print(): void; }
export class Doc implements Printable {
    print(): void {}
}",
    );
    println!("PROBE implements:\n{output}");
    assert!(
        output.contains("implements Printable"),
        "Missing implements: {output}"
    );
}

#[test]
fn probe_class_extends_with_generics() {
    let output = emit_dts(
        "class Base<T> { value: T; }
export class Derived extends Base<string> {
    extra: number;
}",
    );
    println!("PROBE extends generic:\n{output}");
    assert!(
        output.contains("extends Base<string>"),
        "Missing generic extends: {output}"
    );
}

#[test]
fn probe_mapped_type_modifiers() {
    let output = emit_dts("export type T = { readonly [K in string]: number };");
    println!("PROBE mapped readonly:\n{output}");
    assert!(
        output.contains("readonly [K in string]"),
        "Missing readonly mapped: {output}"
    );
}

#[test]
fn probe_mapped_type_minus_modifier() {
    let output = emit_dts("export type T<U> = { -readonly [K in keyof U]-?: U[K] };");
    println!("PROBE mapped minus:\n{output}");
    assert!(output.contains("-readonly"), "Missing -readonly: {output}");
    assert!(output.contains("-?"), "Missing -?: {output}");
}

#[test]
fn probe_infer_with_extends_constraint() {
    let output = emit_dts("export type T<U> = U extends (infer V extends string) ? V : never;");
    println!("PROBE infer extends:\n{output}");
    assert!(
        output.contains("infer V extends string"),
        "Missing infer extends: {output}"
    );
}

#[test]
fn probe_class_method_overloads() {
    let output = emit_dts(
        "export class Foo {
    bar(x: string): number;
    bar(x: number): string;
    bar(x: any): any { return x; }
}",
    );
    println!("PROBE method overloads:\n{output}");
    assert!(
        output.contains("bar(x: string): number;"),
        "Missing overload 1: {output}"
    );
    assert!(
        output.contains("bar(x: number): string;"),
        "Missing overload 2: {output}"
    );
    assert!(
        !output.contains("x: any): any"),
        "Implementation leaked: {output}"
    );
}

#[test]
fn probe_export_star_as_namespace() {
    let output = emit_dts("export * as ns from './mod';");
    println!("PROBE star as ns:\n{output}");
    assert!(
        output.contains("export * as ns from"),
        "Missing star-as: {output}"
    );
}

#[test]
fn probe_type_only_reexport() {
    let output = emit_dts("export type { Foo, Bar } from './mod';");
    println!("PROBE type reexport:\n{output}");
    assert!(
        output.contains("export type {") || output.contains("export type{"),
        "Missing type reexport: {output}"
    );
}

#[test]
fn probe_default_type_parameter() {
    let output = emit_dts("export type T<U = string> = U[];");
    println!("PROBE default type param:\n{output}");
    assert!(
        output.contains("U = string"),
        "Missing default type param: {output}"
    );
}

// =============================================================================
// Edge case probes — compare exactly against tsc output
// =============================================================================

#[test]
fn probe_class_static_method() {
    let output = emit_dts(
        "export class Foo {
    static create(): Foo { return new Foo(); }
}",
    );
    println!("PROBE static method:\n{output}");
    assert!(
        output.contains("static create(): Foo;") || output.contains("static create():"),
        "Missing static method: {output}"
    );
}

#[test]
fn probe_class_protected_abstract_method() {
    let output = emit_dts(
        "export abstract class Base {
    protected abstract init(): void;
}",
    );
    println!("PROBE protected abstract:\n{output}");
    assert!(
        output.contains("protected abstract init(): void;"),
        "Missing protected abstract: {output}"
    );
}

#[test]
fn probe_readonly_property_in_interface() {
    let output = emit_dts(
        "export interface Foo {
    readonly bar: string;
}",
    );
    println!("PROBE readonly prop:\n{output}");
    assert!(
        output.contains("readonly bar: string;"),
        "Missing readonly: {output}"
    );
}

#[test]
fn probe_optional_method_in_interface() {
    let output = emit_dts(
        "export interface Foo {
    bar?(x: number): void;
}",
    );
    println!("PROBE optional method:\n{output}");
    assert!(
        output.contains("bar?(x: number): void;"),
        "Missing optional method: {output}"
    );
}

#[test]
fn probe_export_default_type_alias() {
    // tsc emits: export default T; (with a separate `type T = ...;` if needed)
    // Actually, `export default` on a type alias is not valid TS syntax
    // Let's test `export default interface` instead
    let output = emit_dts(
        "export default interface Foo {
    x: number;
}",
    );
    println!("PROBE default interface:\n{output}");
    assert!(
        output.contains("export default interface Foo"),
        "Missing default interface: {output}"
    );
}

#[test]
fn probe_enum_string_values() {
    let output = emit_dts(
        "export enum Status {
    Active = 'active',
    Inactive = 'inactive'
}",
    );
    println!("PROBE enum string:\n{output}");
    assert!(
        output.contains("Active = \"active\"") || output.contains("Active = 'active'"),
        "Missing string value: {output}"
    );
}

#[test]
fn probe_enum_computed_values() {
    let output = emit_dts(
        "export enum Bits {
    A = 1,
    B = 2,
    C = A | B
}",
    );
    println!("PROBE enum computed:\n{output}");
    // tsc evaluates constant expressions
    assert!(
        output.contains("C = 3") || output.contains("C ="),
        "Missing computed enum: {output}"
    );
}

#[test]
fn probe_class_with_index_signature() {
    let output = emit_dts(
        "export class Foo {
    [key: string]: any;
    bar: number;
}",
    );
    println!("PROBE class index sig:\n{output}");
    assert!(
        output.contains("[key: string]: any;"),
        "Missing index sig: {output}"
    );
    assert!(output.contains("bar: number;"), "Missing bar: {output}");
}

#[test]
fn probe_ambient_enum() {
    let output = emit_dts("export declare enum E { A, B, C }");
    println!("PROBE ambient enum:\n{output}");
    assert!(
        output.contains("export declare enum E"),
        "Missing declare enum: {output}"
    );
}

#[test]
fn probe_never_return_type() {
    let output = emit_dts("export function fail(msg: string): never;");
    println!("PROBE never return:\n{output}");
    assert!(
        output.contains(": never;"),
        "Missing never return: {output}"
    );
}

#[test]
fn probe_symbol_type() {
    let output = emit_dts("export declare const s: symbol;");
    println!("PROBE symbol type:\n{output}");
    assert!(output.contains(": symbol;"), "Missing symbol: {output}");
}

#[test]
fn probe_variadic_tuple() {
    let output =
        emit_dts("export type Concat<T extends unknown[], U extends unknown[]> = [...T, ...U];");
    println!("PROBE variadic tuple:\n{output}");
    assert!(
        output.contains("[...T, ...U]"),
        "Missing variadic: {output}"
    );
}

#[test]
fn probe_type_alias_with_type_literal() {
    let output = emit_dts("export type Obj = { a: string; b: number; };");
    println!("PROBE type alias literal:\n{output}");
    assert!(output.contains("a: string;"), "Missing a: {output}");
    assert!(output.contains("b: number;"), "Missing b: {output}");
}

#[test]
fn probe_nested_namespace() {
    let output = emit_dts(
        "export namespace A {
    export namespace B {
        export function foo(): void;
    }
}",
    );
    println!("PROBE nested ns:\n{output}");
    assert!(output.contains("namespace A"), "Missing A: {output}");
    assert!(output.contains("namespace B"), "Missing B: {output}");
    assert!(
        output.contains("function foo(): void;"),
        "Missing foo: {output}"
    );
}

#[test]
fn probe_const_assertion_variable() {
    // `as const` variables should emit the literal type
    let output = emit_dts("export const x = 42 as const;");
    println!("PROBE as const:\n{output}");
    // Should have `x: 42` not `x: number`
    // Without type inference, it may just emit `any` or the initializer
    println!("Output: {output}");
}

#[test]
fn probe_export_namespace_with_type_and_value() {
    let output = emit_dts(
        "export namespace NS {
    export interface I { x: number; }
    export function f(): I;
    export const c: number;
}",
    );
    println!("PROBE ns type+value:\n{output}");
    assert!(
        output.contains("interface I"),
        "Missing interface I: {output}"
    );
    assert!(
        output.contains("function f(): I;"),
        "Missing fn f: {output}"
    );
    assert!(
        output.contains("const c: number;"),
        "Missing const c: {output}"
    );
}

#[test]
fn probe_global_augmentation() {
    let output = emit_dts(
        "export {};
declare global {
    interface Window {
        myProp: string;
    }
}",
    );
    println!("PROBE global augmentation:\n{output}");
    assert!(
        output.contains("declare global"),
        "Missing global: {output}"
    );
    assert!(
        output.contains("interface Window"),
        "Missing Window: {output}"
    );
}

#[test]
fn probe_function_with_this_param() {
    let output = emit_dts("export function foo(this: HTMLElement, x: number): void;");
    println!("PROBE this param:\n{output}");
    assert!(
        output.contains("this: HTMLElement"),
        "Missing this param: {output}"
    );
}

#[test]
fn probe_class_constructor_overloads() {
    let output = emit_dts(
        "export class Foo {
    constructor(x: string);
    constructor(x: number);
    constructor(x: any) {}
}",
    );
    println!("PROBE ctor overloads:\n{output}");
    assert!(
        output.contains("constructor(x: string);"),
        "Missing ctor overload 1: {output}"
    );
    assert!(
        output.contains("constructor(x: number);"),
        "Missing ctor overload 2: {output}"
    );
    assert!(
        !output.contains("x: any)"),
        "Ctor implementation leaked: {output}"
    );
}

#[test]
fn probe_rest_parameter() {
    let output = emit_dts("export function foo(...args: string[]): void;");
    println!("PROBE rest param:\n{output}");
    assert!(
        output.contains("...args: string[]"),
        "Missing rest param: {output}"
    );
}

#[test]
fn probe_optional_parameter() {
    let output = emit_dts("export function foo(x?: number): void;");
    println!("PROBE optional param:\n{output}");
    assert!(
        output.contains("x?: number"),
        "Missing optional param: {output}"
    );
}

#[test]
fn probe_parameter_with_default() {
    let output = emit_dts("export function foo(x: number = 42): void;");
    println!("PROBE param default:\n{output}");
    // In .d.ts, default values should make param optional: `x?: number`
    assert!(
        output.contains("x?: number"),
        "Default param should be optional: {output}"
    );
}

#[test]
fn probe_class_accessor_keyword() {
    // TS 4.9+ accessor keyword
    let output = emit_dts(
        "export class Foo {
    accessor bar: string = '';
}",
    );
    println!("PROBE accessor keyword:\n{output}");
    // tsc emits: `accessor bar: string;`
    assert!(
        output.contains("accessor bar: string;"),
        "Missing accessor keyword: {output}"
    );
}

#[test]
fn probe_satisfies_stripped() {
    // satisfies should be stripped in .d.ts
    let output = emit_dts("export const x = { a: 1 } satisfies Record<string, number>;");
    println!("PROBE satisfies stripped:\n{output}");
    assert!(
        !output.contains("satisfies"),
        "satisfies should be stripped: {output}"
    );
}

#[test]
fn probe_private_constructor() {
    let output = emit_dts(
        "export class Singleton {
    private constructor() {}
    static instance: Singleton;
}",
    );
    println!("PROBE private ctor:\n{output}");
    assert!(
        output.contains("private constructor();"),
        "Missing private ctor: {output}"
    );
}

#[test]
fn probe_js_class_like_prototype_heuristic() {
    let output = emit_js_dts(
        "let Dog;\nDog.prototype.bark = function() { return 'woof'; };\nDog.prototype.age = 1;",
    );
    println!("PROBE js class-like heuristic:\n{output}");
    assert!(
        output.contains("declare class Dog"),
        "missing class emit: {output}"
    );
    assert!(
        output.contains("private constructor();"),
        "missing private constructor: {output}"
    );
    assert!(
        output.contains("bark()"),
        "missing prototype method: {output}"
    );
    assert!(
        output.contains("age: number;"),
        "missing prototype value: {output}"
    );
}

#[test]
fn probe_abstract_class_with_protected_constructor() {
    let output = emit_dts(
        "export abstract class Base {
    protected constructor(x: number);
}",
    );
    println!("PROBE protected ctor:\n{output}");
    assert!(
        output.contains("protected constructor(x: number);"),
        "Missing protected ctor: {output}"
    );
}

// =============================================================================
// Exact output comparison probes to find subtle differences with tsc
// =============================================================================

#[test]
fn exact_probe_method_with_optional_and_rest() {
    let output =
        emit_dts("export declare function foo(a: string, b?: number, ...rest: boolean[]): void;");
    println!("EXACT method opt+rest:\n{output}");
    // tsc: export declare function foo(a: string, b?: number, ...rest: boolean[]): void;
    let expected =
        "export declare function foo(a: string, b?: number, ...rest: boolean[]): void;\n";
    assert_eq!(output, expected, "Mismatch");
}

#[test]
fn exact_probe_type_alias_union() {
    let output = emit_dts("export type T = string | number | boolean;");
    println!("EXACT type alias union:\n{output}");
    let expected = "export type T = string | number | boolean;\n";
    assert_eq!(output, expected, "Mismatch");
}

#[test]
fn exact_probe_export_default_function_no_name() {
    let output = emit_dts("export default function(): void {}");
    println!("EXACT default fn no name:\n{output}");
    // tsc: export default function (): void;\n
    assert!(
        output.contains("export default function"),
        "Missing default fn: {output}"
    );
}

#[test]
fn exact_probe_export_default_class_no_name() {
    let output = emit_dts("export default class { foo(): void {} }");
    println!("EXACT default class no name:\n{output}");
    assert!(
        output.contains("export default class"),
        "Missing default class: {output}"
    );
}

#[test]
fn exact_probe_async_function() {
    let output = emit_dts("export async function foo(): Promise<number> { return 42; }");
    println!("EXACT async fn:\n{output}");
    // tsc strips async in .d.ts
    assert!(
        !output.contains("async"),
        "async should be stripped in .d.ts: {output}"
    );
    assert!(
        output.contains("foo(): Promise<number>;"),
        "Missing return type: {output}"
    );
}

#[test]
fn exact_probe_generator_function() {
    let output = emit_dts("export function* gen(): Generator<number> { yield 1; }");
    println!("EXACT generator fn:\n{output}");
    // tsc strips * in .d.ts
    assert!(
        !output.contains("*"),
        "* should be stripped in .d.ts: {output}"
    );
}

#[test]
fn exact_probe_class_extends_implements_combined() {
    let output = emit_dts(
        "interface I { foo(): void; }
class Base { bar(): void {} }
export class Derived extends Base implements I {
    foo(): void {}
    bar(): void {}
}",
    );
    println!("EXACT extends+implements:\n{output}");
    assert!(
        output.contains("extends Base implements I"),
        "Missing extends+implements: {output}"
    );
}

#[test]
fn exact_probe_multiline_type_literal_in_alias() {
    let output = emit_dts(
        "export type Obj = {
    a: string;
    b: number;
    c: boolean;
};",
    );
    println!("EXACT multiline type literal:\n{output}");
    // tsc emits multi-line format:
    // export type Obj = {
    //     a: string;
    //     b: number;
    //     c: boolean;
    // };
    assert!(output.contains("a: string;"), "Missing a: {output}");
    assert!(output.contains("b: number;"), "Missing b: {output}");
    assert!(output.contains("c: boolean;"), "Missing c: {output}");
}

#[test]
fn exact_probe_complex_mapped_type() {
    let output = emit_dts(
        "export type Required<T> = {
    [P in keyof T]-?: T[P];
};",
    );
    println!("EXACT complex mapped:\n{output}");
    assert!(
        output.contains("[P in keyof T]-?: T[P]"),
        "Missing mapped type body: {output}"
    );
}

#[test]
fn exact_probe_intersection_of_unions() {
    let output = emit_dts("export type T = (string | number) & (boolean | null);");
    println!("EXACT intersection of unions:\n{output}");
    assert!(
        output.contains("(string | number) & (boolean | null)"),
        "Missing parens in intersection: {output}"
    );
}

#[test]
fn exact_probe_nested_conditional() {
    let output = emit_dts(
        "export type T<U> = U extends string ? 'str' : U extends number ? 'num' : 'other';",
    );
    println!("EXACT nested conditional:\n{output}");
    assert!(
        output.contains("U extends string"),
        "Missing outer extends: {output}"
    );
    assert!(
        output.contains("U extends number"),
        "Missing inner extends: {output}"
    );
}

#[test]
fn exact_probe_typeof_import() {
    let output = emit_dts("export type T = typeof import('./mod');");
    println!("EXACT typeof import:\n{output}");
    assert!(
        output.contains("typeof import"),
        "Missing typeof import: {output}"
    );
}

#[test]
fn exact_probe_class_with_optional_property() {
    let output = emit_dts(
        "export class Foo {
    bar?: string;
    baz!: number;
}",
    );
    println!("EXACT optional + definite:\n{output}");
    assert!(
        output.contains("bar?: string;"),
        "Missing optional prop: {output}"
    );
    // tsc strips ! definite assignment in .d.ts
    assert!(
        output.contains("baz: number;") || output.contains("baz!: number;"),
        "Missing definite prop: {output}"
    );
    // tsc emits `baz!: number;` in .d.ts? Actually no - tsc strips the `!`
    // Let me check if we strip the `!` token
}

#[test]
fn exact_probe_declare_abstract_class() {
    let output = emit_dts(
        "export declare abstract class Base {
    abstract method(): void;
    concrete(): string;
}",
    );
    println!("EXACT declare abstract:\n{output}");
    let expected = "export declare abstract class Base {\n    abstract method(): void;\n    concrete(): string;\n}\n";
    assert_eq!(output, expected, "Mismatch");
}

#[test]
fn exact_probe_interface_with_generic_method() {
    let output = emit_dts(
        "export interface Foo {
    bar<T>(x: T): T;
}",
    );
    println!("EXACT generic method:\n{output}");
    assert!(
        output.contains("bar<T>(x: T): T;"),
        "Missing generic method: {output}"
    );
}

#[test]
fn exact_probe_function_type_in_union() {
    let output = emit_dts("export type T = ((x: number) => void) | string;");
    println!("EXACT fn type in union:\n{output}");
    // The parentheses around the function type should be preserved
    assert!(
        output.contains("((x: number) => void)") || output.contains("(x: number) => void"),
        "Missing fn type: {output}"
    );
}

#[test]
fn exact_probe_this_type() {
    let output = emit_dts(
        "export class Builder {
    set(key: string): this;
}",
    );
    println!("EXACT this type:\n{output}");
    assert!(
        output.contains("set(key: string): this;"),
        "Missing this type: {output}"
    );
}

#[test]
fn exact_probe_string_index_signature() {
    let output = emit_dts(
        "export interface Dict {
    [key: string]: unknown;
}",
    );
    println!("EXACT string index:\n{output}");
    assert!(
        output.contains("[key: string]: unknown;"),
        "Missing index sig: {output}"
    );
}

#[test]
fn exact_probe_number_index_signature() {
    let output = emit_dts(
        "export interface Arr {
    [index: number]: string;
}",
    );
    println!("EXACT number index:\n{output}");
    assert!(
        output.contains("[index: number]: string;"),
        "Missing index sig: {output}"
    );
}
