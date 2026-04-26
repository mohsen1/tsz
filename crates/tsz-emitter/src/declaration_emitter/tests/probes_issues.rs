use super::*;

// =========================================================================
// Probe tests for finding DTS emit issues
// =========================================================================

#[test]
fn probe_dts_issue_conditional_in_union() {
    // Conditional type as a union member must be parenthesized:
    // `string | T extends U ? X : Y` parses as `(string | T) extends U ? X : Y`
    // which is different from `string | (T extends U ? X : Y)`
    let output = emit_dts("export type X = string | (T extends string ? number : boolean);");
    println!("PROBE conditional in union:\n{output}");
    assert!(
        output.contains("string | (T extends string ? number : boolean)"),
        "Conditional in union needs parens: {output}"
    );
}

#[test]
fn test_conditional_type_in_intersection_parenthesized() {
    // Conditional type as an intersection member must be parenthesized:
    // `A & T extends U ? X : Y` parses differently without parens
    let output = emit_dts(
        "export type Y = { a: string } & (T extends string ? { b: number } : { c: boolean });",
    );
    println!("conditional in intersection:\n{output}");
    assert!(
        output.contains("(T extends string ?"),
        "Conditional in intersection needs parens: {output}"
    );
}

#[test]
fn probe_dts_issue_typeof_import() {
    let output = emit_dts("export declare const x: typeof import('./foo');");
    println!("PROBE typeof import:\n{output}");
    assert!(output.contains("typeof import("), "typeof import: {output}");
}

#[test]
fn probe_dts_issue_bigint_literal_type() {
    let output = emit_dts("export declare const x: 100n;");
    println!("PROBE bigint literal type:\n{output}");
    assert!(output.contains("100n"), "bigint literal type: {output}");
}

#[test]
fn probe_dts_issue_negative_number_literal() {
    let output = emit_dts("export declare const x: -1;");
    println!("PROBE negative literal:\n{output}");
    assert!(output.contains("-1"), "negative number literal: {output}");
}

#[test]
fn probe_dts_issue_declare_global() {
    let output = emit_dts(
        "export {};
declare global {
    interface Window {
        foo: string;
    }
}",
    );
    println!("PROBE declare global:\n{output}");
    assert!(
        output.contains("declare global"),
        "declare global: {output}"
    );
    assert!(
        output.contains("foo: string"),
        "declare global members: {output}"
    );
}

#[test]
fn probe_dts_issue_export_enum_member_computed() {
    // Enum with computed property name using string
    let output = emit_dts(
        "export declare enum E {
    A = 0,
    B = 1,
    C = 2
}",
    );
    println!("PROBE enum:\n{output}");
    assert!(output.contains("A = 0"), "enum member A: {output}");
}

#[test]
fn probe_dts_issue_class_with_accessor_keyword() {
    let output = emit_dts(
        "export declare class Foo {
    accessor name: string;
}",
    );
    println!("PROBE accessor keyword:\n{output}");
    assert!(
        output.contains("accessor name: string"),
        "accessor keyword: {output}"
    );
}

#[test]
fn probe_dts_issue_satisfies_stripped() {
    // satisfies in initializer - should be stripped, type inferred
    let output = emit_dts("export const x = { a: 1 } satisfies Record<string, number>;");
    println!("PROBE satisfies:\n{output}");
    // Satisfies should be stripped from DTS output
    assert!(
        !output.contains("satisfies"),
        "satisfies should be stripped: {output}"
    );
}

#[test]
fn probe_dts_issue_as_const() {
    // as const in initializer - should emit readonly types
    let output = emit_dts("export const x = [1, 2, 3] as const;");
    println!("PROBE as const:\n{output}");
    assert!(
        !output.contains("as const"),
        "as const should be stripped: {output}"
    );
}

#[test]
fn probe_dts_issue_void_function_expression_return() {
    // void keyword used as expression operator
    let output = emit_dts("export declare function foo(): void;");
    println!("PROBE void return:\n{output}");
    assert!(output.contains("): void;"), "void return: {output}");
}

#[test]
fn probe_dts_issue_nested_template_literal_type() {
    let output = emit_dts("export type Nested = `${`inner${string}`}outer`;");
    println!("PROBE nested template literal:\n{output}");
    assert!(output.contains("`"), "template literal: {output}");
}

#[test]
fn probe_dts_issue_class_implements_multiple() {
    let output = emit_dts(
        "export declare class Foo implements A, B, C {
    a: number;
}",
    );
    println!("PROBE implements multiple:\n{output}");
    assert!(
        output.contains("implements A, B, C"),
        "implements multiple: {output}"
    );
}

#[test]
fn probe_dts_issue_export_type_star() {
    let output = emit_dts("export type * from './foo';");
    println!("PROBE export type star:\n{output}");
    assert!(
        output.contains("export type * from"),
        "export type star: {output}"
    );
}

#[test]
fn probe_dts_issue_export_type_star_as_ns() {
    let output = emit_dts("export type * as ns from './foo';");
    println!("PROBE export type * as ns:\n{output}");
    assert!(
        output.contains("export type * as ns from") || output.contains("export type *"),
        "export type * as ns: {output}"
    );
}

#[test]
fn probe_dts_issue_import_type_with_qualifier() {
    let output = emit_dts("export declare const x: import('./foo').Bar.Baz;");
    println!("PROBE import type qualifier:\n{output}");
    assert!(
        output.contains("import("),
        "import type with qualifier: {output}"
    );
}

#[test]
fn probe_dts_issue_const_enum() {
    let output = emit_dts(
        "export const enum Direction {
    Up = 0,
    Down = 1,
    Left = 2,
    Right = 3
}",
    );
    println!("PROBE const enum:\n{output}");
    assert!(
        output.contains("const enum Direction"),
        "const enum: {output}"
    );
}

#[test]
fn probe_dts_issue_ambient_enum() {
    let output = emit_dts(
        "export declare enum Direction {
    Up,
    Down,
    Left,
    Right
}",
    );
    println!("PROBE ambient enum:\n{output}");
    assert!(output.contains("enum Direction"), "ambient enum: {output}");
    assert!(output.contains("Up"), "ambient enum members: {output}");
}

#[test]
fn probe_dts_class_with_declare_field() {
    let output = emit_dts(
        "export class Foo {
    declare bar: string;
}",
    );
    println!("PROBE declare field:\n{output}");
    // declare fields should be emitted in .d.ts
    assert!(
        output.contains("bar: string") || output.contains("bar:"),
        "declare field: {output}"
    );
}

#[test]
fn probe_dts_rest_tuple_type() {
    let output =
        emit_dts("export declare function foo(...args: [string, ...number[], boolean]): void;");
    println!("PROBE rest tuple type:\n{output}");
    assert!(
        output.contains("[string, ...number[], boolean]"),
        "rest tuple type: {output}"
    );
}

#[test]
fn probe_dts_declare_module_with_export() {
    let output = emit_dts(
        r#"declare module "foo" {
    export function bar(): void;
    export const baz: number;
}"#,
    );
    println!("PROBE declare module:\n{output}");
    assert!(
        output.contains("declare module"),
        "declare module: {output}"
    );
    // Inside declare module, functions should NOT have declare keyword
    let module_body = &output[output.find('{').unwrap()..];
    println!("Module body: {module_body}");
    assert!(
        !module_body.contains("declare function"),
        "Should not have 'declare' inside module body: {output}"
    );
}

#[test]
fn probe_dts_abstract_constructor_signatures() {
    let output = emit_dts("export type MixinConstructor = abstract new (...args: any[]) => any;");
    println!("PROBE abstract constructor sig:\n{output}");
    assert!(
        output.contains("abstract new"),
        "abstract constructor: {output}"
    );
}

#[test]
fn probe_dts_tuple_labeled_optional_rest() {
    let output = emit_dts("export type T = [first: string, second?: number, ...rest: boolean[]];");
    println!("PROBE labeled tuple:\n{output}");
    assert!(output.contains("first: string"), "first label: {output}");
    assert!(
        output.contains("second?: number"),
        "optional label: {output}"
    );
    assert!(
        output.contains("...rest: boolean[]"),
        "rest label: {output}"
    );
}

#[test]
fn probe_dts_tuple_optional_rest_unlabeled() {
    // The (invalid) tuple form `[...T?]` is parsed by tsc as a rest element
    // wrapping an optional inner type, and printed as `[...?T]` in declaration
    // emit. Regression cover for restTupleElements1 (T09).
    let output = emit_dts("export type T = [...string?];");
    println!("PROBE rest+optional tuple:\n{output}");
    assert!(output.contains("[...?string]"), "rest+optional: {output}");
    // A trailing `?` on a non-rest tuple element must remain `[T?]`.
    let output2 = emit_dts("export type U = [string?];");
    println!("PROBE optional tuple:\n{output2}");
    assert!(output2.contains("[string?]"), "optional only: {output2}");
    // A bare rest element keeps its existing `[...T]` form.
    let output3 = emit_dts("export type V = [...string[]];");
    assert!(output3.contains("[...string[]]"), "rest only: {output3}");
}

#[test]
fn probe_dts_mapped_type_as_clause() {
    let output =
        emit_dts("export type MappedWithAs<T> = { [K in keyof T as `get${string & K}`]: T[K] };");
    println!("PROBE mapped with as:\n{output}");
    assert!(
        output.contains("as `get${string & K}`"),
        "mapped type as clause: {output}"
    );
}

#[test]
fn probe_dts_overloaded_function_export() {
    let output = emit_dts(
        "export function foo(x: string): string;
export function foo(x: number): number;
export function foo(x: any): any { return x; }",
    );
    println!("PROBE overloaded function:\n{output}");
    let foo_count = output.matches("function foo").count();
    assert_eq!(
        foo_count, 2,
        "Should emit exactly 2 overload signatures, got {foo_count}: {output}"
    );
}

#[test]
fn probe_dts_constructor_type_in_type_alias() {
    let output = emit_dts("export type Ctor<T> = new (...args: any[]) => T;");
    println!("PROBE constructor type:\n{output}");
    assert!(
        output.contains("new (...args: any[]) => T"),
        "constructor type: {output}"
    );
}

#[test]
fn probe_dts_intersection_with_function() {
    let output = emit_dts("export type F = ((x: string) => void) & { bar: number };");
    println!("PROBE intersection with function:\n{output}");
    assert!(
        output.contains("((x: string) => void) & {"),
        "intersection with function: {output}"
    );
}

#[test]
fn probe_dts_conditional_type_infer() {
    let output = emit_dts("export type UnpackPromise<T> = T extends Promise<infer U> ? U : T;");
    println!("PROBE conditional infer:\n{output}");
    assert!(output.contains("infer U"), "conditional infer: {output}");
    assert!(
        output.contains("T extends Promise<infer U> ? U : T"),
        "conditional structure: {output}"
    );
}

#[test]
fn probe_dts_index_signature_readonly() {
    let output = emit_dts(
        "export interface ReadonlyMap {
    readonly [key: string]: number;
}",
    );
    println!("PROBE readonly index sig:\n{output}");
    assert!(
        output.contains("readonly [key: string]: number"),
        "readonly index sig: {output}"
    );
}

#[test]
fn probe_dts_optional_property_in_type_literal() {
    let output =
        emit_dts("export declare const x: { a: string; b?: number; readonly c: boolean };");
    println!("PROBE type literal props:\n{output}");
    assert!(output.contains("a: string"), "regular prop: {output}");
    assert!(output.contains("b?: number"), "optional prop: {output}");
    assert!(
        output.contains("readonly c: boolean"),
        "readonly prop: {output}"
    );
}

#[test]
fn probe_dts_export_namespace_from() {
    let output = emit_dts("export * as ns from './foo';");
    println!("PROBE export ns from:\n{output}");
    assert!(
        output.contains("export * as ns from"),
        "export namespace from: {output}"
    );
}

#[test]
fn probe_dts_class_with_static_member() {
    let output = emit_dts(
        "export declare class Foo {
    static bar: string;
    static baz(): void;
}",
    );
    println!("PROBE static members:\n{output}");
    assert!(
        output.contains("static bar: string"),
        "static property: {output}"
    );
    assert!(
        output.contains("static baz(): void"),
        "static method: {output}"
    );
}

#[test]
fn probe_dts_class_with_protected() {
    let output = emit_dts(
        "export declare class Foo {
    protected bar: string;
    protected baz(): void;
}",
    );
    println!("PROBE protected members:\n{output}");
    assert!(
        output.contains("protected bar: string"),
        "protected property: {output}"
    );
    assert!(
        output.contains("protected baz(): void"),
        "protected method: {output}"
    );
}

#[test]
fn probe_dts_generic_constraint_with_default() {
    let output = emit_dts("export declare function foo<T extends object = {}>(x: T): T;");
    println!("PROBE generic constraint+default:\n{output}");
    assert!(
        output.contains("T extends object = {}"),
        "generic constraint+default: {output}"
    );
}

#[test]
fn probe_dts_string_enum() {
    let output = emit_dts(
        r#"export enum Color {
    Red = "RED",
    Blue = "BLUE"
}"#,
    );
    println!("PROBE string enum:\n{output}");
    assert!(
        output.contains(r#"Red = "RED""#),
        "string enum member: {output}"
    );
}

#[test]
fn probe_dts_bigint_literal_expression() {
    let output = emit_dts("export declare const x: 42n;");
    println!("PROBE bigint literal:\n{output}");
    // BigInt literal types should be preserved
    assert!(output.contains("42n"), "bigint literal: {output}");
}

// =====================================================================
// More edge-case probes
// =====================================================================

#[test]
fn probe_dts_conditional_type_in_array() {
    // Conditional type as array element needs parens
    let output = emit_dts("export type X = (T extends string ? number : boolean)[];");
    println!("PROBE conditional in array:\n{output}");
    assert!(
        output.contains("(T extends string ? number : boolean)[]"),
        "Conditional in array needs parens: {output}"
    );
}

#[test]
fn probe_dts_function_type_in_array() {
    let output = emit_dts("export type X = ((x: number) => string)[];");
    println!("PROBE function in array:\n{output}");
    assert!(
        output.contains("((x: number) => string)[]"),
        "Function in array needs parens: {output}"
    );
}

#[test]
fn probe_dts_union_in_array() {
    let output = emit_dts("export type X = (string | number)[];");
    println!("PROBE union in array:\n{output}");
    assert!(
        output.contains("(string | number)[]"),
        "Union in array needs parens: {output}"
    );
}

#[test]
fn probe_dts_typeof_in_union() {
    let output = emit_dts("export declare const x: string | typeof Array;");
    println!("PROBE typeof in union:\n{output}");
    assert!(output.contains("typeof Array"), "typeof in union: {output}");
}

#[test]
fn probe_dts_keyof_in_conditional() {
    let output = emit_dts("export type X<T> = keyof T extends string ? T : never;");
    println!("PROBE keyof in conditional:\n{output}");
    assert!(
        output.contains("keyof T extends string"),
        "keyof in conditional: {output}"
    );
}

#[test]
fn probe_dts_readonly_array_type() {
    let output = emit_dts("export declare const x: readonly string[];");
    println!("PROBE readonly array:\n{output}");
    assert!(
        output.contains("readonly string[]"),
        "readonly array: {output}"
    );
}

#[test]
fn probe_dts_readonly_tuple_type() {
    let output = emit_dts("export declare const x: readonly [string, number];");
    println!("PROBE readonly tuple:\n{output}");
    assert!(
        output.contains("readonly [string, number]"),
        "readonly tuple: {output}"
    );
}

#[test]
fn probe_dts_type_assertion_in_extends_clause() {
    // In extends clause, complex expressions should be handled
    let output = emit_dts("export declare class Foo extends Array<string> { }");
    println!("PROBE extends generic:\n{output}");
    assert!(
        output.contains("extends Array<string>"),
        "extends generic: {output}"
    );
}

#[test]
fn probe_dts_infer_type_with_extends() {
    let output =
        emit_dts("export type GetString<T> = T extends { a: infer U extends string } ? U : never;");
    println!("PROBE infer extends:\n{output}");
    assert!(
        output.contains("infer U extends string"),
        "infer extends: {output}"
    );
}

#[test]
fn probe_dts_multiple_call_signatures() {
    let output = emit_dts(
        "export interface Callable {
    (x: string): string;
    (x: number): number;
}",
    );
    println!("PROBE multiple call sigs:\n{output}");
    let count = output.matches("(x:").count();
    assert_eq!(count, 2, "Should have 2 call signatures: {output}");
}

#[test]
fn probe_dts_construct_signature() {
    let output = emit_dts(
        "export interface Newable {
    new (x: string): object;
}",
    );
    println!("PROBE construct sig:\n{output}");
    assert!(
        output.contains("new (x: string): object"),
        "construct signature: {output}"
    );
}

#[test]
fn probe_dts_symbol_computed_property() {
    let output = emit_dts(
        "export interface Iterable {
    [Symbol.iterator](): Iterator<any>;
}",
    );
    println!("PROBE symbol computed:\n{output}");
    assert!(
        output.contains("[Symbol.iterator]"),
        "symbol computed property: {output}"
    );
}

#[test]
fn probe_dts_generator_function_return() {
    let output = emit_dts("export declare function* gen(): Generator<number, void, undefined>;");
    println!("PROBE generator:\n{output}");
    // Generator functions in .d.ts should NOT have the * (it goes into the return type)
    // Actually tsc strips the * and keeps Generator return type
    assert!(
        output.contains("Generator<number, void, undefined>"),
        "generator return type: {output}"
    );
}

#[test]
fn probe_dts_template_literal_with_union() {
    let output = emit_dts(r#"export type EventName = `${"click" | "focus"}_handler`;"#);
    println!("PROBE template literal union:\n{output}");
    assert!(output.contains("`"), "template literal: {output}");
}

#[test]
fn probe_dts_nested_generic_types() {
    let output = emit_dts("export declare const x: Map<string, Set<Array<number>>>;");
    println!("PROBE nested generics:\n{output}");
    assert!(
        output.contains("Map<string, Set<Array<number>>>"),
        "nested generics: {output}"
    );
}

#[test]
fn probe_dts_class_with_private_constructor() {
    let output = emit_dts(
        "export class Singleton {
    private constructor();
}",
    );
    println!("PROBE private constructor:\n{output}");
    assert!(
        output.contains("private constructor()"),
        "private constructor: {output}"
    );
}

#[test]
fn probe_dts_export_import_equals() {
    let output = emit_dts(
        "import foo = require('foo');
export = foo;",
    );
    println!("PROBE export import equals:\n{output}");
    assert!(output.contains("export = foo"), "export equals: {output}");
}

#[test]
fn probe_dts_type_alias_with_recursive_type() {
    let output = emit_dts(
        "export type Json = string | number | boolean | null | Json[] | { [key: string]: Json };",
    );
    println!("PROBE recursive type:\n{output}");
    assert!(output.contains("Json[]"), "recursive type: {output}");
}

#[test]
fn probe_dts_generator_star_stripped() {
    // Generator function declarations strip the `*` in .d.ts
    let output = emit_dts("export function* myGen(): Generator<number, string, boolean> {}");
    println!("PROBE generator star:\n{output}");
    assert!(
        !output.contains("function*"),
        "generator star should be stripped: {output}"
    );
    assert!(
        output.contains("function myGen"),
        "generator name preserved: {output}"
    );
}

#[test]
fn probe_dts_async_function_stripped() {
    // async keyword should be stripped in .d.ts (return type encodes Promise)
    let output = emit_dts("export async function myAsync(): Promise<void> {}");
    println!("PROBE async stripped:\n{output}");
    assert!(
        !output.contains("async"),
        "async should be stripped: {output}"
    );
}

#[test]
fn probe_dts_class_private_method_with_types() {
    // Private methods in .d.ts should omit types
    let output = emit_dts(
        "export declare class Foo {
    private bar(x: number): string;
}",
    );
    println!("PROBE private method:\n{output}");
    assert!(
        output.contains("private bar;"),
        "private method should be property-like: {output}"
    );
    assert!(
        !output.contains("private bar("),
        "private method should not have params: {output}"
    );
}

#[test]
fn probe_dts_never_type() {
    let output = emit_dts("export declare function throwError(): never;");
    println!("PROBE never:\n{output}");
    assert!(output.contains("): never;"), "never return type: {output}");
}

#[test]
fn probe_dts_interface_with_string_index() {
    let output = emit_dts(
        "export interface Dict<T> {
    [key: string]: T;
}",
    );
    println!("PROBE string index:\n{output}");
    assert!(
        output.contains("[key: string]: T"),
        "string index: {output}"
    );
}

#[test]
fn probe_dts_class_with_optional_method() {
    let output = emit_dts(
        "export declare class Foo {
    bar?(x: number): string;
}",
    );
    println!("PROBE optional method:\n{output}");
    assert!(output.contains("bar?"), "optional method: {output}");
}

#[test]
fn probe_dts_mapped_type_minus_readonly() {
    let output = emit_dts("export type Mutable<T> = { -readonly [P in keyof T]: T[P] };");
    println!("PROBE -readonly mapped:\n{output}");
    assert!(
        output.contains("-readonly"),
        "-readonly mapped type: {output}"
    );
}

#[test]
fn probe_dts_mapped_type_minus_optional() {
    let output = emit_dts("export type Required<T> = { [P in keyof T]-?: T[P] };");
    println!("PROBE -? mapped:\n{output}");
    assert!(output.contains("-?"), "-? mapped type: {output}");
}

#[test]
fn probe_dts_export_default_expression_value() {
    // export default with expression should synthesize a variable
    let output = emit_dts("export default 42;");
    println!("PROBE export default expr:\n{output}");
    assert!(
        output.contains("export default"),
        "export default: {output}"
    );
}

#[test]
fn probe_dts_const_assertion_value() {
    // `as const` on a value - should be stripped
    let output = emit_dts("export const arr = [1, 2, 3] as const;");
    println!("PROBE const assertion value:\n{output}");
    assert!(
        !output.contains("as const"),
        "as const should be stripped from value: {output}"
    );
}

#[test]
fn probe_dts_function_with_destructured_param() {
    let output = emit_dts("export function foo({ a, b }: { a: number; b: string }): void {}");
    println!("PROBE destructured param:\n{output}");
    assert!(
        output.contains("{ a, b }"),
        "destructured param pattern: {output}"
    );
    assert!(
        output.contains("a: number"),
        "destructured param type: {output}"
    );
}

#[test]
fn probe_dts_function_with_rest_param() {
    let output = emit_dts("export function foo(a: number, ...rest: string[]): void {}");
    println!("PROBE rest param:\n{output}");
    assert!(output.contains("...rest: string[]"), "rest param: {output}");
}

#[test]
fn probe_dts_function_with_default_param() {
    let output = emit_dts("export function foo(x: number = 42): void {}");
    println!("PROBE default param:\n{output}");
    // Default params become optional in .d.ts
    assert!(
        output.contains("x?: number"),
        "default param becomes optional: {output}"
    );
}

#[test]
fn probe_dts_interface_method_overloads() {
    let output = emit_dts(
        "export interface Converter {
    convert(x: string): number;
    convert(x: number): string;
}",
    );
    println!("PROBE interface method overloads:\n{output}");
    let count = output.matches("convert(").count();
    assert_eq!(count, 2, "Should have 2 method overloads: {output}");
}

#[test]
fn probe_dts_using_declaration() {
    // `using` declarations should emit as `const` in .d.ts
    let output = emit_dts("export using x: Disposable = getResource();");
    println!("PROBE using decl:\n{output}");
    assert!(
        output.contains("const x"),
        "using should emit as const: {output}"
    );
}

#[test]
fn probe_dts_keyof_with_parens() {
    // `keyof (A | B)` needs parens to be different from `keyof A | B`
    let output = emit_dts("export type X = keyof (A | B);");
    println!("PROBE keyof with parens:\n{output}");
    assert!(
        output.contains("keyof (A | B)"),
        "keyof should preserve parens around union: {output}"
    );
}

#[test]
fn probe_dts_conditional_type_nested() {
    // Nested conditionals are right-associative in false branch
    let output = emit_dts(
        "export type X<T> = T extends string ? 'str' : T extends number ? 'num' : 'other';",
    );
    println!("PROBE nested conditional:\n{output}");
    assert!(
        output.contains("T extends string"),
        "nested conditional first part: {output}"
    );
    assert!(
        output.contains("T extends number"),
        "nested conditional second part: {output}"
    );
}

// =====================================================================
// Edge case probes - round 3
// =====================================================================

#[test]
fn probe_dts_optional_type_with_conditional() {
    // Optional tuple element with conditional type needs parens
    let output = emit_dts("export type X = [(T extends string ? number : boolean)?];");
    println!("PROBE optional conditional:\n{output}");
    assert!(
        output.contains("(T extends string ? number : boolean)?"),
        "optional conditional needs parens: {output}"
    );
}

#[test]
fn probe_dts_conditional_type_in_indexed_access() {
    let output = emit_dts("export type X = (string | number)['toString'];");
    println!("PROBE union in indexed access:\n{output}");
    assert!(
        output.contains("(string | number)["),
        "union in indexed access needs parens: {output}"
    );
}

#[test]
fn probe_dts_array_of_conditional() {
    let output = emit_dts("export type X = (T extends string ? number : boolean)[];");
    println!("PROBE array of conditional:\n{output}");
    assert!(
        output.contains("(T extends string ? number : boolean)[]"),
        "array of conditional needs parens: {output}"
    );
}

#[test]
fn probe_dts_function_in_union() {
    let output = emit_dts("export type X = string | ((x: number) => void);");
    println!("PROBE function in union:\n{output}");
    assert!(
        output.contains("((x: number) => void)"),
        "function type in union needs parens: {output}"
    );
}

#[test]
fn probe_dts_constructor_type_in_union() {
    let output = emit_dts("export type X = string | (new (x: number) => object);");
    println!("PROBE constructor in union:\n{output}");
    assert!(
        output.contains("(new (x: number) => object)"),
        "constructor type in union needs parens: {output}"
    );
}

#[test]
fn probe_dts_conditional_in_conditional_extends() {
    // Conditional type in extends position of another conditional
    let output = emit_dts(
        "export type X<T> = T extends (U extends string ? number : boolean) ? 'yes' : 'no';",
    );
    println!("PROBE conditional in extends:\n{output}");
    assert!(
        output.contains("(U extends string ? number : boolean)"),
        "conditional in extends position needs parens: {output}"
    );
}

#[test]
fn probe_dts_type_operator_keyof_union() {
    // keyof should bind tighter than union; `keyof (A | B)` needs parens
    let output = emit_dts("export type X = keyof (A | B);");
    println!("PROBE keyof union:\n{output}");
    assert!(
        output.contains("keyof (A | B)"),
        "keyof union needs parens: {output}"
    );
}

#[test]
fn probe_dts_readonly_union() {
    // readonly should bind tighter than union; `readonly (A | B)` needs parens
    let output = emit_dts("export type X = readonly (string | number)[];");
    println!("PROBE readonly union array:\n{output}");
    assert!(
        output.contains("readonly (string | number)[]"),
        "readonly union array: {output}"
    );
}

#[test]
fn probe_dts_infer_type_in_conditional() {
    let output =
        emit_dts("export type ElementType<T> = T extends readonly (infer U)[] ? U : never;");
    println!("PROBE infer in array:\n{output}");
    assert!(output.contains("infer U"), "infer type: {output}");
}

#[test]
fn probe_dts_generic_defaults_complex() {
    let output =
        emit_dts("export type Foo<A extends object = {}, B extends keyof A = keyof A> = A[B];");
    println!("PROBE complex generic defaults:\n{output}");
    assert!(
        output.contains("B extends keyof A = keyof A"),
        "complex generic defaults: {output}"
    );
}

#[test]
fn probe_dts_variance_modifiers() {
    let output = emit_dts("export interface Container<in out T> { value: T; }");
    println!("PROBE variance modifiers:\n{output}");
    assert!(output.contains("in out T"), "variance modifiers: {output}");
}

#[test]
fn probe_dts_const_type_param() {
    let output =
        emit_dts("export declare function foo<const T extends readonly string[]>(args: T): T;");
    println!("PROBE const type param:\n{output}");
    assert!(
        output.contains("const T extends readonly string[]"),
        "const type param: {output}"
    );
}

#[test]
fn probe_dts_negative_bigint_literal() {
    let output = emit_dts("export declare const x: -100n;");
    println!("PROBE negative bigint:\n{output}");
    assert!(
        output.contains("-100n"),
        "negative bigint literal: {output}"
    );
}

#[test]
fn probe_dts_union_in_optional_type() {
    // Union in optional tuple member needs parens
    let output = emit_dts("export type X = [(string | number)?];");
    println!("PROBE union in optional:\n{output}");
    assert!(
        output.contains("(string | number)?"),
        "union in optional type: {output}"
    );
}

#[test]
fn probe_dts_intersection_in_optional_type() {
    let output = emit_dts("export type X = [(A & B)?];");
    println!("PROBE intersection in optional:\n{output}");
    assert!(
        output.contains("(A & B)?"),
        "intersection in optional type: {output}"
    );
}

#[test]
fn probe_dts_function_in_conditional_check() {
    // Function type as check type of conditional needs parens
    let output = emit_dts("export type X = ((x: number) => void) extends Function ? true : false;");
    println!("PROBE function in conditional check:\n{output}");
    assert!(
        output.contains("((x: number) => void) extends Function"),
        "function in conditional check: {output}"
    );
}

#[test]
fn probe_dts_union_in_conditional_check() {
    let output = emit_dts("export type X = (string | number) extends object ? true : false;");
    println!("PROBE union in conditional check:\n{output}");
    assert!(
        output.contains("(string | number) extends object"),
        "union in conditional check: {output}"
    );
}

#[test]
fn test_this_type_in_type_position() {
    // `this` as a type uses the parser's THIS_TYPE node kind (198),
    // not ThisKeyword (110). Both must be handled.
    let output = emit_dts(
        "export interface Chainable {
    chain(): this;
    map(f: (x: this) => this): this;
}",
    );
    println!("this type:\n{output}");
    assert!(
        output.contains("chain(): this"),
        "this return type: {output}"
    );
    assert!(
        output.contains("(x: this) => this"),
        "this in function type: {output}"
    );
}

#[test]
fn test_this_type_in_type_alias() {
    // `this` type in type alias
    let output = emit_dts("export type SelfRef = { value: this };");
    println!("this in type alias:\n{output}");
    assert!(
        output.contains("value: this"),
        "this in type literal: {output}"
    );
}

#[test]
fn test_conditional_type_in_indexed_access() {
    // Conditional type as object of indexed access needs parens
    // Without: T extends U ? X : Y[K] -> parses [K] as indexing Y only
    // With: (T extends U ? X : Y)[K] -> indexes the whole conditional
    let output = emit_dts(
        "export type X<T, K extends string> = (T extends string ? { a: number } : { b: string })[K];",
    );
    println!("conditional in indexed access:\n{output}");
    assert!(
        output.contains("(T extends string ?"),
        "conditional in indexed access needs parens: {output}"
    );
}
