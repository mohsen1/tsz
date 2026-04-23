use super::*;

// =============================================================================
// Edge case tests: comprehensive tsc-parity verification
// =============================================================================

#[test]
fn test_abstract_accessors() {
    let result = emit_dts(
        r#"
export abstract class AbstractBase {
    abstract get name(): string;
    abstract set name(value: string);
}
"#,
    );
    assert!(
        result.contains("abstract get name(): string;"),
        "Missing abstract getter: {result}"
    );
    assert!(
        result.contains("abstract set name(value: string);"),
        "Missing abstract setter: {result}"
    );
}

#[test]
fn test_overloaded_constructors() {
    let result = emit_dts(
        r#"
export class OverloadedCtor {
    constructor(x: number);
    constructor(x: string, y: number);
    constructor(x: number | string, y?: number) {}
}
"#,
    );
    assert!(
        result.contains("constructor(x: number);"),
        "Missing first overload: {result}"
    );
    assert!(
        result.contains("constructor(x: string, y: number);"),
        "Missing second overload: {result}"
    );
    assert!(
        !result.contains("constructor(x: number | string"),
        "Implementation should be omitted: {result}"
    );
}

#[test]
fn test_complex_generic_constraints_with_defaults() {
    let result = emit_dts(
        r#"
export type Complex<T extends Record<string, unknown> = Record<string, any>, U extends keyof T = keyof T> = {
    [K in U]: T[K];
};
"#,
    );
    assert!(
        result.contains("Record<string, unknown>"),
        "Missing constraint: {result}"
    );
    assert!(
        result.contains("Record<string, any>"),
        "Missing default: {result}"
    );
    assert!(
        result.contains("keyof T"),
        "Missing keyof constraint: {result}"
    );
}

#[test]
fn test_intersection_with_call_signatures() {
    let result = emit_dts(
        r#"
export type Callable = { (): void } & { (x: number): number } & { name: string };
"#,
    );
    assert!(
        result.contains("(): void"),
        "Missing first call sig: {result}"
    );
    assert!(
        result.contains("(x: number): number"),
        "Missing second call sig: {result}"
    );
    assert!(
        result.contains("name: string"),
        "Missing property: {result}"
    );
}

#[test]
fn test_recursive_type_alias() {
    let result = emit_dts(
        r#"
export type Tree<T> = {
    value: T;
    children: Tree<T>[];
};
"#,
    );
    assert!(
        result.contains("Tree<T>[]"),
        "Missing recursive ref: {result}"
    );
}

#[test]
fn test_const_enum_edge_case() {
    let result = emit_dts(
        r#"
export const enum Color {
    Red = 1,
    Green = 2,
    Blue = 4,
}
"#,
    );
    assert!(
        result.contains("const enum Color"),
        "Missing const enum: {result}"
    );
    assert!(result.contains("Red = 1"), "Missing Red: {result}");
    assert!(result.contains("Green = 2"), "Missing Green: {result}");
}

#[test]
fn test_class_index_signatures_edge() {
    let result = emit_dts(
        r#"
export class IndexClass {
    [key: string]: any;
}
"#,
    );
    assert!(
        result.contains("[key: string]: any;"),
        "Missing index sig: {result}"
    );
}

#[test]
fn test_mixed_parameter_properties() {
    let result = emit_dts(
        r#"
export class MixedParams {
    constructor(
        public readonly x: number,
        protected y: string,
        private z: boolean,
        public w?: number,
    ) {}
}
"#,
    );
    // In d.ts, parameter properties become both constructor params and property declarations
    assert!(
        result.contains("readonly x: number"),
        "Missing readonly property: {result}"
    );
}

#[test]
fn test_conditional_type_with_infer() {
    let result = emit_dts(
        r#"
export type UnwrapPromise<T> = T extends Promise<infer U> ? U : T;
"#,
    );
    assert!(result.contains("infer U"), "Missing infer: {result}");
    assert!(
        result.contains("Promise<infer U>"),
        "Missing Promise<infer U>: {result}"
    );
}

#[test]
fn test_module_augmentation() {
    let result = emit_dts(
        r#"
declare module "express" {
    interface Request {
        userId?: string;
    }
}
"#,
    );
    assert!(
        result.contains("declare module \"express\""),
        "Missing module augmentation: {result}"
    );
    assert!(
        result.contains("userId?"),
        "Missing augmented property: {result}"
    );
}

#[test]
fn test_global_augmentation() {
    let result = emit_dts(
        r#"
export {};
declare global {
    interface Window {
        customProp: number;
    }
}
"#,
    );
    assert!(
        result.contains("declare global"),
        "Missing global augmentation: {result}"
    );
    assert!(
        result.contains("customProp: number"),
        "Missing global property: {result}"
    );
}

#[test]
fn test_readonly_tuple_with_rest_and_labels() {
    let result = emit_dts(
        r#"
export type ReadonlyTuple = readonly [first: string, ...rest: number[]];
"#,
    );
    assert!(
        result.contains("readonly ["),
        "Missing readonly tuple: {result}"
    );
    assert!(
        result.contains("first: string"),
        "Missing labeled element: {result}"
    );
    assert!(
        result.contains("...rest: number[]"),
        "Missing rest element: {result}"
    );
}

#[test]
fn test_template_literal_type() {
    let result = emit_dts(
        r#"
export type Greeting<T extends string> = `Hello, ${T}!`;
"#,
    );
    assert!(
        result.contains("${T}"),
        "Missing template literal type: {result}"
    );
}

#[test]
fn test_import_type() {
    let result = emit_dts(
        r#"
export type LazyModule = typeof import("fs");
"#,
    );
    assert!(
        result.contains("import(\"fs\")"),
        "Missing import type: {result}"
    );
}

#[test]
fn test_namespace_with_class_and_interface() {
    let result = emit_dts(
        r#"
export namespace Shapes {
    export class Circle {
        radius: number;
    }
    export interface Circle {
        area(): number;
    }
}
"#,
    );
    assert!(
        result.contains("namespace Shapes"),
        "Missing namespace: {result}"
    );
    assert!(result.contains("class Circle"), "Missing class: {result}");
    assert!(
        result.contains("interface Circle"),
        "Missing interface: {result}"
    );
}

#[test]
fn test_symbol_iterator_method() {
    let result = emit_dts(
        r#"
export class IterableClass {
    *[Symbol.iterator](): Iterator<number> {
        yield 1;
    }
}
"#,
    );
    assert!(
        result.contains("[Symbol.iterator]"),
        "Missing Symbol.iterator: {result}"
    );
    assert!(
        result.contains("Iterator<number>"),
        "Missing return type: {result}"
    );
}

#[test]
fn test_getter_only() {
    let result = emit_dts(
        r#"
export class GetOnly {
    get value(): number { return 42; }
}
"#,
    );
    assert!(
        result.contains("get value(): number;"),
        "Missing getter: {result}"
    );
    assert!(
        !result.contains("set value"),
        "Should not have setter: {result}"
    );
}

#[test]
fn test_fluent_this_return_type() {
    let result = emit_dts(
        r#"
export class FluentBuilder {
    setName(name: string): this {
        return this;
    }
}
"#,
    );
    assert!(
        result.contains("setName(name: string): this;"),
        "Missing this return type: {result}"
    );
}

#[test]
fn test_mapped_type_with_as_clause() {
    let result = emit_dts(
        r#"
export type EventMap<T extends string> = {
    [K in T as `on${Capitalize<K>}`]: (event: K) => void;
};
"#,
    );
    assert!(
        result.contains("as `on${Capitalize<K>}`"),
        "Missing as clause with template: {result}"
    );
}

#[test]
fn test_abstract_method_with_generics() {
    let result = emit_dts(
        r#"
export abstract class AbstractGeneric<T> {
    abstract transform<U>(input: T): U;
}
"#,
    );
    assert!(
        result.contains("abstract transform<U>(input: T): U;"),
        "Missing abstract generic method: {result}"
    );
}

#[test]
fn debug_print_mixed_params_vs_tsc() {
    let result = emit_dts(
        r#"
export class MixedParams {
    constructor(
        public readonly x: number,
        protected y: string,
        private z: boolean,
        public w?: number,
    ) {}
}
"#,
    );
    // tsc outputs:
    // export declare class MixedParams {
    //     readonly x: number;
    //     protected y: string;
    //     private z;
    //     w?: number | undefined;
    //     constructor(x: number, y: string, z: boolean, w?: number | undefined);
    // }
    let expected = "export declare class MixedParams {\n    readonly x: number;\n    protected y: string;\n    private z;\n    w?: number | undefined;\n    constructor(x: number, y: string, z: boolean, w?: number | undefined);\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn debug_print_const_enum_vs_tsc() {
    // tsc omits trailing comma on last enum member
    let result = emit_dts(
        r#"
export const enum Color {
    Red = 1,
    Green = 2,
    Blue = 4,
}
"#,
    );
    let expected =
        "export declare const enum Color {\n    Red = 1,\n    Green = 2,\n    Blue = 4\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn debug_print_global_augmentation_vs_tsc() {
    let result = emit_dts(
        r#"
export {};
declare global {
    interface Window {
        customProp: number;
    }
}
"#,
    );
    let expected = "export {};\ndeclare global {\n    interface Window {\n        customProp: number;\n    }\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_enum_computed_bitwise_values_vs_tsc() {
    // tsc evaluates bitwise expressions: 1<<0 -> 1, 1<<1 -> 2, Read|Write|Execute -> 7
    let result = emit_dts(
        r#"
export enum Flags {
    None = 0,
    Read = 1 << 0,
    Write = 1 << 1,
    Execute = 1 << 2,
    All = Read | Write | Execute,
}
"#,
    );
    let expected = "export declare enum Flags {\n    None = 0,\n    Read = 1,\n    Write = 2,\n    Execute = 4,\n    All = 7\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_mixed_string_numeric_enum_vs_tsc() {
    let result = emit_dts(
        r#"
export enum Mixed {
    A = 0,
    B = "hello",
    C = 1,
}
"#,
    );
    let expected = "export declare enum Mixed {\n    A = 0,\n    B = \"hello\",\n    C = 1\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_function_overloads_vs_tsc() {
    let result = emit_dts(
        r#"
export function overloaded(x: number): number;
export function overloaded(x: string): string;
export function overloaded(x: number | string): number | string {
    return x;
}
"#,
    );
    let expected = "export declare function overloaded(x: number): number;\nexport declare function overloaded(x: string): string;\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_default_export_interface_vs_tsc() {
    let result = emit_dts(
        r#"
export default interface Config {
    port: number;
    host: string;
}
"#,
    );
    let expected = "export default interface Config {\n    port: number;\n    host: string;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_construct_signature_interface_vs_tsc() {
    let result = emit_dts(
        r#"
export interface Constructor<T> {
    new (...args: any[]): T;
    prototype: T;
}
"#,
    );
    let expected =
        "export interface Constructor<T> {\n    new (...args: any[]): T;\n    prototype: T;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_namespace_function_overloads_vs_tsc() {
    let result = emit_dts(
        r#"
export declare namespace MyLib {
    function create(tag: "div"): HTMLDivElement;
    function create(tag: "span"): HTMLSpanElement;
    function create(tag: string): HTMLElement;
}
"#,
    );
    let expected = "export declare namespace MyLib {\n    function create(tag: \"div\"): HTMLDivElement;\n    function create(tag: \"span\"): HTMLSpanElement;\n    function create(tag: string): HTMLElement;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_interface_multiple_extends_vs_tsc() {
    let result = emit_dts(
        r#"
export interface A { a: number; }
export interface B { b: string; }
export interface C extends A, B { c: boolean; }
"#,
    );
    let expected = "export interface A {\n    a: number;\n}\nexport interface B {\n    b: string;\n}\nexport interface C extends A, B {\n    c: boolean;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_negative_enum_values_vs_tsc() {
    let result = emit_dts(
        r#"
export enum NegativeEnum {
    A = -1,
    B = -2,
    C = -100,
}
"#,
    );
    let expected =
        "export declare enum NegativeEnum {\n    A = -1,\n    B = -2,\n    C = -100\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_unique_symbol_and_computed_property_vs_tsc() {
    let result = emit_dts(
        r#"
export declare const sym: unique symbol;
export interface Keyed {
    [sym]: string;
}
"#,
    );
    let expected = "export declare const sym: unique symbol;\nexport interface Keyed {\n    [sym]: string;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_this_parameter_in_method_vs_tsc() {
    let result = emit_dts(
        r#"
export class Guard {
    isValid(this: Guard): boolean { return true; }
}
"#,
    );
    let expected = "export declare class Guard {\n    isValid(this: Guard): boolean;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_variadic_tuple_types_vs_tsc() {
    let result = emit_dts(
        r#"
export type Concat<T extends readonly unknown[], U extends readonly unknown[]> = [...T, ...U];
"#,
    );
    let expected = "export type Concat<T extends readonly unknown[], U extends readonly unknown[]> = [...T, ...U];\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_numeric_literal_interface_keys_vs_tsc() {
    let result = emit_dts(
        r#"
export interface NumberKeyed {
    0: string;
    1: number;
    2: boolean;
}
"#,
    );
    let expected =
        "export interface NumberKeyed {\n    0: string;\n    1: number;\n    2: boolean;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_private_fields_vs_tsc() {
    let result = emit_dts(
        r#"
export class PrivateFields {
    #name: string;
    #age: number;
    constructor(name: string, age: number) {
        this.#name = name;
        this.#age = age;
    }
    getName(): string { return this.#name; }
}
"#,
    );
    // tsc collapses all #private fields into a single `#private;` declaration
    let expected = "export declare class PrivateFields {\n    #private;\n    constructor(name: string, age: number);\n    getName(): string;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_static_block_omitted_vs_tsc() {
    let result = emit_dts(
        r#"
export class WithStaticBlock {
    static value: number;
    static {
        WithStaticBlock.value = 42;
    }
    method(): void {}
}
"#,
    );
    // tsc omits static blocks from .d.ts
    let expected = "export declare class WithStaticBlock {\n    static value: number;\n    method(): void;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_enum_inside_namespace_vs_tsc() {
    let result = emit_dts(
        r#"
export namespace NS {
    export enum Status {
        Active = "active",
        Inactive = "inactive",
    }
}
"#,
    );
    // tsc: no trailing comma on last member
    let expected = "export declare namespace NS {\n    enum Status {\n        Active = \"active\",\n        Inactive = \"inactive\"\n    }\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_optional_methods_in_interface_vs_tsc() {
    let result = emit_dts(
        r#"
export interface Events {
    on?(event: string, handler: Function): void;
    off?(event: string, handler: Function): void;
}
"#,
    );
    let expected = "export interface Events {\n    on?(event: string, handler: Function): void;\n    off?(event: string, handler: Function): void;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_dual_accessor_types_vs_tsc() {
    // TS 5.1+: getter and setter can have different types
    let result = emit_dts(
        r#"
export class DualAccessor {
    get value(): string { return ""; }
    set value(v: string | number) {}
}
"#,
    );
    let expected = "export declare class DualAccessor {\n    get value(): string;\n    set value(v: string | number);\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_overloaded_call_signatures_in_interface_vs_tsc() {
    let result = emit_dts(
        r#"
export interface Converter {
    (input: string): number;
    (input: number): string;
    name: string;
}
"#,
    );
    let expected = "export interface Converter {\n    (input: string): number;\n    (input: number): string;\n    name: string;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_nested_conditional_type_vs_tsc() {
    let result = emit_dts(
        r#"
export type DeepReadonly<T> = T extends (infer U)[] ? DeepReadonly<U>[] : T extends object ? { readonly [K in keyof T]: DeepReadonly<T[K]> } : T;
"#,
    );
    // tsc reformats to multiline for the mapped type portion
    let expected = "export type DeepReadonly<T> = T extends (infer U)[] ? DeepReadonly<U>[] : T extends object ? {\n    readonly [K in keyof T]: DeepReadonly<T[K]>;\n} : T;\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_enum_with_string_member_names_vs_tsc() {
    let result = emit_dts(
        r#"
export enum StringKeys {
    "hello world" = 0,
    "foo-bar" = 1,
}
"#,
    );
    let expected =
        "export declare enum StringKeys {\n    \"hello world\" = 0,\n    \"foo-bar\" = 1\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_export_equals_namespace_vs_tsc() {
    let result = emit_dts(
        r#"
declare namespace MyLib {
    interface Config {
        value: number;
    }
    function create(): Config;
}
export = MyLib;
"#,
    );
    let expected = "declare namespace MyLib {\n    interface Config {\n        value: number;\n    }\n    function create(): Config;\n}\nexport = MyLib;\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_nested_destructured_param_vs_tsc() {
    let result = emit_dts(
        r#"
export function process(
    { a, b: { c }, ...rest }: { a: number; b: { c: string }; d: boolean; e: number }
): void {}
"#,
    );
    // tsc preserves the destructured pattern and reformats the type to multiline
    let expected = "export declare function process({ a, b: { c }, ...rest }: {\n    a: number;\n    b: {\n        c: string;\n    };\n    d: boolean;\n    e: number;\n}): void;\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_float_enum_values_vs_tsc() {
    let result = emit_dts(
        r#"
export enum FloatEnum {
    Half = 0.5,
    Quarter = 0.25,
    Pi = 3.14159,
}
"#,
    );
    let expected = "export declare enum FloatEnum {\n    Half = 0.5,\n    Quarter = 0.25,\n    Pi = 3.14159\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_this_parameter_in_function_vs_tsc() {
    let result = emit_dts(
        r#"
export declare function handler(this: HTMLElement, event: Event): void;
"#,
    );
    let expected = "export declare function handler(this: HTMLElement, event: Event): void;\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_generic_interface_with_conditional_default_vs_tsc() {
    let result = emit_dts(
        r#"
export interface Mapper<Input = unknown, Output = Input extends string ? number : boolean> {
    map(input: Input): Output;
}
"#,
    );
    let expected = "export interface Mapper<Input = unknown, Output = Input extends string ? number : boolean> {\n    map(input: Input): Output;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_overloaded_generic_methods_in_interface_vs_tsc() {
    let result = emit_dts(
        r#"
export interface Factory {
    create<T>(type: new () => T): T;
    create<T, U>(type: new (arg: U) => T, arg: U): T;
}
"#,
    );
    let expected = "export interface Factory {\n    create<T>(type: new () => T): T;\n    create<T, U>(type: new (arg: U) => T, arg: U): T;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_infer_type_with_extends_constraint_vs_tsc() {
    // TS 4.7+: infer C extends string
    let result = emit_dts(
        r#"
export type FirstChar<T extends string> = T extends `${infer C extends string}${string}` ? C : never;
"#,
    );
    let expected = "export type FirstChar<T extends string> = T extends `${infer C extends string}${string}` ? C : never;\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_mapped_type_minus_readonly_minus_optional_vs_tsc() {
    let result = emit_dts(
        r#"
export type Mutable<T> = {
    -readonly [K in keyof T]-?: T[K];
};
export type ReadonlyOptional<T> = {
    +readonly [K in keyof T]+?: T[K];
};
"#,
    );
    let expected = "export type Mutable<T> = {\n    -readonly [K in keyof T]-?: T[K];\n};\nexport type ReadonlyOptional<T> = {\n    +readonly [K in keyof T]+?: T[K];\n};\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_indexed_access_type_with_mapped_vs_tsc() {
    let result = emit_dts(
        r#"
export type KeysOfType<T, V> = { [K in keyof T]: T[K] extends V ? K : never }[keyof T];
"#,
    );
    // tsc reformats to multi-line
    let expected = "export type KeysOfType<T, V> = {\n    [K in keyof T]: T[K] extends V ? K : never;\n}[keyof T];\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_named_tuple_with_optional_element_vs_tsc() {
    let result = emit_dts(
        r#"
export type NamedTuple = [first: string, second?: number, ...rest: boolean[]];
"#,
    );
    let expected =
        "export type NamedTuple = [first: string, second?: number, ...rest: boolean[]];\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_enum_string_concat_vs_tsc() {
    // tsc evaluates string concatenation: "prefix_" + "a" -> "prefix_a"
    let result = emit_dts(
        r#"
export enum StringEnum {
    A = "prefix_" + "a",
}
"#,
    );
    let expected = "export declare enum StringEnum {\n    A = \"prefix_a\"\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_interface_string_literal_keys_vs_tsc() {
    let result = emit_dts(
        r#"
export interface StringKeyed {
    "hello world": number;
    "with-dash": string;
    "with space": boolean;
    normal: number;
}
"#,
    );
    let expected = "export interface StringKeyed {\n    \"hello world\": number;\n    \"with-dash\": string;\n    \"with space\": boolean;\n    normal: number;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_class_extends_implements_vs_tsc() {
    let result = emit_dts(
        r#"
export interface Printable {
    print(): void;
}
export interface Loggable {
    log(): void;
}
export class Base {
    id: number = 0;
}
export class Derived extends Base implements Printable, Loggable {
    print(): void {}
    log(): void {}
}
"#,
    );
    let expected = "export interface Printable {\n    print(): void;\n}\nexport interface Loggable {\n    log(): void;\n}\nexport declare class Base {\n    id: number;\n}\nexport declare class Derived extends Base implements Printable, Loggable {\n    print(): void;\n    log(): void;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_overloaded_callable_type_vs_tsc() {
    let result = emit_dts(
        r#"
export type OverloadedFn = {
    (x: string): string;
    (x: number): number;
    readonly length: number;
};
"#,
    );
    let expected = "export type OverloadedFn = {\n    (x: string): string;\n    (x: number): number;\n    readonly length: number;\n};\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_import_type_equals_require_preserves_type_keyword() {
    // `export import type X = require("module")` must preserve the `type` keyword in .d.ts
    let output = emit_dts(r#"export import type Foo = require("some-module");"#);
    assert!(
        output.contains("import type Foo = require("),
        "import type equals should preserve 'type' keyword: {output}"
    );
}

#[test]
fn test_import_equals_require_without_type() {
    // Regular `import X = require("module")` should NOT have `type` keyword
    let output = emit_dts_with_usage_analysis(
        r#"
import Foo = require("some-module");
export declare function useFoo(): Foo;
"#,
    );
    // When usage analysis is active, the non-exported import may be elided
    // unless it's actually referenced. The key assertion is that if it IS
    // emitted, it does NOT have "type" in it.
    if output.contains("import ") && output.contains("= require(") {
        assert!(
            !output.contains("import type Foo"),
            "Regular import equals should not have 'type' keyword: {output}"
        );
    }
}

#[test]
fn test_export_import_type_equals_require() {
    // `export import type X = require("module")` - exported type-only import equals
    let output = emit_dts(r#"export import type Bar = require("bar-module");"#);
    assert!(
        output.contains("import type Bar = require("),
        "export import type equals should preserve 'type' keyword: {output}"
    );
}

#[test]
fn test_import_defer_preserves_keyword() {
    // `import defer * as ns from "mod"` must preserve the `defer` keyword in .d.ts
    let output = emit_dts_with_usage_analysis(
        r#"
import defer * as ns from "./mod";
export declare function useMod(): typeof ns;
"#,
    );
    // If the import is emitted (not elided), it should have defer
    if output.contains("import ") && output.contains("* as ns") {
        assert!(
            output.contains("import defer * as ns"),
            "import defer should preserve 'defer' keyword: {output}"
        );
    }
}

#[test]
fn test_accessor_keyword_preserved_on_class_field() {
    // TypeScript `accessor` keyword (auto-accessor) should be preserved in .d.ts
    let output = emit_dts(
        r#"export class Foo {
    accessor name: string;
    static accessor count: number;
}"#,
    );
    assert!(
        output.contains("accessor name: string;"),
        "accessor keyword should be preserved: {output}"
    );
    assert!(
        output.contains("static accessor count: number;"),
        "static accessor keyword should be preserved: {output}"
    );
}

#[test]
fn test_object_literal_shorthand_function_emits_typeof() {
    // Regression: shorthand `{ doSomethingWithKeys }` where the value is a
    // function symbol must emit `typeof doSomethingWithKeys`, not the
    // expanded function signature. Mirrors tsc's
    // declarationEmitIndexTypeArray baseline.
    let output = emit_dts_with_binding(
        r#"
function doSomethingWithKeys<T>(...keys: (keyof T)[]) { }

const utilityFunctions = {
  doSomethingWithKeys
};
"#,
    );
    assert!(
        output.contains("typeof doSomethingWithKeys"),
        "shorthand property referencing a function value must emit `typeof`: {output}"
    );
    assert!(
        !output.contains("doSomethingWithKeys: <T>"),
        "expanded generic signature should not appear in place of typeof: {output}"
    );
}

#[test]
fn test_const_enum_computed_method_name_keeps_method_syntax() {
    // Regression: a class method with a const-enum-member computed name
    // (e.g. `[G.A]() {}`) must emit method syntax (`[G.A](): void;`),
    // not property syntax (`[G.A]: () => void;`). The dts predicate that
    // chooses syntax was reading the type cache for a `Literal` form;
    // the binder's `ENUM_MEMBER` symbol flag is now consulted as a
    // fallback so we keep method syntax even when the type system
    // shapes the access as the enum-member type rather than the literal.
    let output = emit_dts_with_binding(
        r#"
const enum G { A = 1, B = 2 }
class C {
    [G.A]() { }
    get [G.B]() { return true; }
    set [G.B](x: number) { }
}
"#,
    );
    assert!(
        output.contains("[G.A](): "),
        "const enum computed method must keep method syntax: {output}"
    );
    assert!(
        !output.contains("[G.A]: () =>"),
        "must not degrade to property syntax for const enum computed method: {output}"
    );
}
