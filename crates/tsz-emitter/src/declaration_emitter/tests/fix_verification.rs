use super::*;

// === Fix verification tests ===

#[test]
fn fix_numeric_separator_stripped_in_type_position() {
    // tsc strips numeric separators in .d.ts output
    let output = emit_dts("export declare const x: 1_000_000;");
    println!("numeric sep:\n{output}");
    assert!(
        output.contains("1000000"),
        "numeric separator should be stripped: {output}"
    );
    assert!(
        !output.contains("1_000_000"),
        "underscore should not appear: {output}"
    );
}

#[test]
fn fix_numeric_separator_hex_with_sep() {
    // tsc converts hex with separators to decimal
    let output = emit_dts("export declare const x: 0xFF_FF;");
    println!("hex sep:\n{output}");
    assert!(
        output.contains("65535"),
        "hex with separator should be decimal 65535: {output}"
    );
}

#[test]
fn fix_numeric_separator_preserved_no_sep() {
    // Without separators, numeric literals should be preserved as-is
    let output = emit_dts("export declare const x: 0xFF;");
    println!("hex no sep:\n{output}");
    assert!(
        output.contains("0xFF"),
        "hex without separator preserved: {output}"
    );
}

#[test]
fn fix_numeric_separator_decimal_no_sep() {
    // Decimal without separator preserved
    let output = emit_dts("export declare const x: 42;");
    println!("decimal no sep:\n{output}");
    assert!(
        output.contains("42"),
        "decimal without separator preserved: {output}"
    );
}

#[test]
fn fix_enum_cross_reference() {
    // tsc computes cross-enum references
    let output = emit_dts("export enum A { X = 1 }\nexport enum B { Y = A.X }");
    println!("enum cross-ref:\n{output}");
    assert!(
        output.contains("Y = 1"),
        "cross-enum ref should be resolved to 1: {output}"
    );
}

#[test]
fn fix_enum_cross_reference_computed() {
    // Cross-enum reference with computation
    let output = emit_dts("export enum A { X = 1, Y = 2 }\nexport enum B { Z = A.X + A.Y }");
    println!("enum cross-ref computed:\n{output}");
    assert!(
        output.contains("Z = 3"),
        "cross-enum ref should compute to 3: {output}"
    );
}

#[test]
fn fix_template_literal_escape_preserved() {
    // Template literal type with escape sequences
    let output = emit_dts(r#"export type T = `hello\nworld`;"#);
    println!("template escape:\n{output}");
    // Should preserve \n as escape sequence, not emit actual newline
    assert!(
        output.contains(r#"`hello\nworld`"#),
        "escape sequence should be preserved: {output}"
    );
    assert!(
        !output.contains("hello\nworld"),
        "actual newline should not appear in template: {output}"
    );
}

#[test]
fn fix_template_literal_simple() {
    // Template literal without escapes should work as before
    let output = emit_dts("export type T = `hello world`;");
    println!("template simple:\n{output}");
    assert!(
        output.contains("`hello world`"),
        "simple template: {output}"
    );
}

#[test]
fn fix_template_literal_with_types() {
    // Template literal with type substitutions
    let output = emit_dts("export type T = `hello ${string}`;");
    println!("template with type:\n{output}");
    assert!(
        output.contains("`hello ${string}`"),
        "template with type: {output}"
    );
}

#[test]
fn fix_numeric_sep_negative() {
    // Negative number with separator
    let output = emit_dts("export declare const x: -1_000;");
    println!("negative sep:\n{output}");
    assert!(
        output.contains("-1000"),
        "negative with separator: {output}"
    );
    assert!(
        !output.contains("_"),
        "underscore should be stripped: {output}"
    );
}

#[test]
fn fix_numeric_sep_binary() {
    // Binary literal with separator
    let output = emit_dts("export declare const x: 0b1010_0101;");
    println!("binary sep:\n{output}");
    // tsc preserves binary notation without separators
    // Actually tsc converts to decimal for non-decimal with separators
    assert!(
        !output.contains("_"),
        "underscore should be stripped: {output}"
    );
}

#[test]
fn fix_numeric_sep_bigint() {
    // BigInt with separator
    let output = emit_dts("export declare const x: 1_000n;");
    println!("bigint sep:\n{output}");
    assert!(
        !output.contains("_"),
        "underscore should be stripped: {output}"
    );
    assert!(
        output.contains("1000n"),
        "bigint separator stripped: {output}"
    );
}

#[test]
fn fix_template_literal_tab_escape() {
    // Template literal with tab escape
    let output = emit_dts(r#"export type T = `hello\tworld`;"#);
    println!("template tab:\n{output}");
    assert!(
        output.contains(r#"`hello\tworld`"#),
        "tab escape preserved: {output}"
    );
}

#[test]
fn fix_template_literal_multi_substitution() {
    // Template literal with multiple type substitutions
    let output = emit_dts("export type T = `${string}-${number}`;");
    println!("template multi sub:\n{output}");
    assert!(
        output.contains("`${string}-${number}`"),
        "multi substitution: {output}"
    );
}

#[test]
fn fix_template_literal_backtick_in_template() {
    // Template literal with escaped backtick
    let output = emit_dts(r#"export type T = `hello\`world`;"#);
    println!("template backtick:\n{output}");
    assert!(
        output.contains(r"`hello\`world`"),
        "escaped backtick: {output}"
    );
}

#[test]
fn fix_enum_self_ref_still_works() {
    // Self-referencing enum should still work
    let output = emit_dts("export enum E { A = 1, B = A + 1, C = A | B }");
    println!("enum self-ref:\n{output}");
    assert!(output.contains("A = 1"), "A = 1: {output}");
    assert!(output.contains("B = 2"), "B = 2: {output}");
    assert!(output.contains("C = 3"), "C = 3: {output}");
}

#[test]
fn dump_const_literal_preservation() {
    let cases = vec![
        ("const-string", "export const a = '1.0';"),
        ("const-number", "export const b = 42;"),
        ("const-boolean", "export const c = true;"),
        ("const-array", "export const d = [1, 2, 3];"),
        ("let-string", "export let e = '1.0';"),
        ("let-number", "export let f = 42;"),
        (
            "static-readonly-string",
            "export class C { static readonly VERSION = '1.0'; }",
        ),
        (
            "static-readonly-number",
            "export class C { static readonly COUNT = 42; }",
        ),
        (
            "static-readonly-bool",
            "export class C { static readonly FLAG = true; }",
        ),
        (
            "static-readonly-array",
            "export class C { static readonly ITEMS = [1, 2, 3]; }",
        ),
        ("static-non-readonly", "export class C { static x = 42; }"),
        ("const-negative", "export const x = -42;"),
        ("const-template", "export const x = `hello`;"),
    ];

    for (label, source) in &cases {
        let output = emit_dts(source);
        println!("=== {label} ===");
        println!("{output}");
        println!();
    }
}

#[test]
fn fix_static_readonly_string_literal_preserved() {
    // tsc preserves literal values for static readonly properties
    let output = emit_dts("export class C { static readonly VERSION = '1.0'; }");
    println!("static readonly string:\n{output}");
    assert!(
        output.contains("= \"1.0\"") || output.contains("= '1.0'"),
        "static readonly string should be preserved as literal: {output}"
    );
}

#[test]
fn fix_static_readonly_number_literal_preserved() {
    let output = emit_dts("export class C { static readonly COUNT = 42; }");
    println!("static readonly number:\n{output}");
    assert!(
        output.contains("= 42"),
        "static readonly number should be preserved: {output}"
    );
}

#[test]
fn fix_static_readonly_boolean_literal_preserved() {
    let output = emit_dts("export class C { static readonly FLAG = true; }");
    println!("static readonly bool:\n{output}");
    assert!(
        output.contains("= true"),
        "static readonly boolean should be preserved: {output}"
    );
}

#[test]
fn fix_static_readonly_array_not_preserved() {
    // Arrays should widen to type, not preserve literal
    let output = emit_dts("export class C { static readonly ITEMS = [1, 2, 3]; }");
    println!("static readonly array:\n{output}");
    // Should NOT have = [...], should have : any[] or similar
    assert!(
        !output.contains("= ["),
        "array should widen, not preserve literal: {output}"
    );
}

#[test]
fn fix_static_non_readonly_widens() {
    // Non-readonly static should widen to type
    let output = emit_dts("export class C { static x = 42; }");
    println!("static non-readonly:\n{output}");
    assert!(
        output.contains(": number"),
        "non-readonly should widen: {output}"
    );
    assert!(
        !output.contains("= 42"),
        "non-readonly should not preserve literal: {output}"
    );
}

#[test]
fn fix_readonly_nonstatic_literal_preserved() {
    // Readonly (non-static) should also preserve literals
    let output = emit_dts("export class C { readonly name = 'hello'; }");
    println!("readonly nonstatic:\n{output}");
    assert!(
        output.contains("= \"hello\"") || output.contains("= 'hello'"),
        "readonly string should be preserved: {output}"
    );
}

#[test]
fn fix_static_readonly_negative_number() {
    let output = emit_dts("export class C { static readonly OFFSET = -42; }");
    println!("static readonly negative:\n{output}");
    assert!(
        output.contains("= -42"),
        "negative number preserved: {output}"
    );
}

#[test]
fn fix_enum_numeric_separator_in_value() {
    // Enum member values with numeric separators should be evaluated correctly
    let output = emit_dts("export enum E { A = 1_000, B = 2_000, C = A + B }");
    println!("enum sep values:\n{output}");
    assert!(output.contains("A = 1000"), "A should be 1000: {output}");
    assert!(output.contains("B = 2000"), "B should be 2000: {output}");
    assert!(output.contains("C = 3000"), "C should be 3000: {output}");
}

#[test]
fn fix_enum_hex_separator_in_value() {
    let output = emit_dts("export enum E { A = 0xFF_FF }");
    println!("enum hex sep:\n{output}");
    assert!(
        output.contains("A = 65535"),
        "hex with sep should evaluate: {output}"
    );
}

#[test]
fn fix_regex_literal_inferred_type() {
    // Regex literal initializer should infer RegExp type
    let output = emit_dts("export const x = /hello/;");
    println!("regex:\n{output}");
    assert!(
        output.contains("RegExp"),
        "regex should infer RegExp: {output}"
    );
}

#[test]
fn fix_template_literal_initializer_inferred_type() {
    // Template literal initializer should infer string type
    let output = emit_dts("export const x = `hello`;");
    println!("template init:\n{output}");
    assert!(
        output.contains("string") || output.contains("\"hello\""),
        "template should infer string: {output}"
    );
}

#[test]
fn fix_template_expression_initializer_inferred_type() {
    let output = emit_dts("export let x = `hello ${42}`;");
    println!("template expr init:\n{output}");
    assert!(
        output.contains("string"),
        "template expression should infer string: {output}"
    );
}

#[test]
fn fix_regex_in_const_with_flags() {
    let output = emit_dts("export const re = /test/gi;");
    println!("regex with flags:\n{output}");
    assert!(
        output.contains("RegExp"),
        "regex with flags should infer RegExp: {output}"
    );
}

#[test]
fn fix_const_numeric_separator_stripped() {
    let output = emit_dts("export const x = 1_000_000;");
    println!("const sep:\n{output}");
    assert!(
        output.contains("1000000"),
        "const numeric sep should be stripped: {output}"
    );
    assert!(
        !output.contains("_"),
        "underscore should not appear: {output}"
    );
}

#[test]
fn fix_const_bigint_separator_stripped() {
    let output = emit_dts("export const x = 1_000n;");
    println!("const bigint sep:\n{output}");
    assert!(
        output.contains("1000n"),
        "bigint sep should be stripped: {output}"
    );
    assert!(
        !output.contains("_"),
        "underscore should not appear: {output}"
    );
}

#[test]
fn fix_const_negative_separator_stripped() {
    let output = emit_dts("export const x = -1_000;");
    println!("const neg sep:\n{output}");
    assert!(
        output.contains("-1000"),
        "negative sep should be stripped: {output}"
    );
    assert!(
        !output.contains("_"),
        "underscore should not appear: {output}"
    );
}

#[test]
fn fix_const_hex_separator_converted() {
    let output = emit_dts("export const x = 0xFF_FF;");
    println!("const hex sep:\n{output}");
    assert!(
        output.contains("65535"),
        "hex sep should convert to decimal: {output}"
    );
}

#[test]
fn fix_numeric_property_name_separator() {
    let output = emit_dts("export interface I { 1_000: string; }");
    println!("numeric prop name sep:\n{output}");
    assert!(
        output.contains("1000:"),
        "numeric property name sep should be stripped: {output}"
    );
    assert!(
        !output.contains("_"),
        "underscore should not appear: {output}"
    );
}

// =============================================================================
// Edge case exploration tests (Round 5 - finding new issues)
// =============================================================================

#[test]
fn explore_static_block_stripped() {
    // Static blocks should be stripped from .d.ts
    let output = emit_dts(
        "export class Foo {
    static x: number;
    static {
        this.x = 42;
    }
    y: string;
}",
    );
    println!("static block:\n{output}");
    assert!(
        !output.contains("static {"),
        "static block should be stripped: {output}"
    );
    assert!(
        output.contains("static x: number;"),
        "static property should remain: {output}"
    );
    assert!(
        output.contains("y: string;"),
        "property should remain: {output}"
    );
}

#[test]
fn explore_import_type_full_syntax() {
    // import("./module").SomeType should be preserved
    let output = emit_dts("export type MyType = import('./module').SomeType;");
    println!("import type:\n{output}");
    assert!(
        output.contains("import("),
        "import type should be preserved: {output}"
    );
}

#[test]
fn explore_class_with_static_block_and_method() {
    let output = emit_dts(
        "export class Counter {
    static count: number;
    static {
        Counter.count = 0;
    }
    increment(): void {
        Counter.count++;
    }
}",
    );
    println!("static block + method:\n{output}");
    assert!(
        !output.contains("static {"),
        "static block should be stripped: {output}"
    );
    assert!(
        output.contains("static count: number;"),
        "static property should remain: {output}"
    );
    assert!(
        output.contains("increment(): void;"),
        "method should remain: {output}"
    );
}

#[test]
fn explore_constructor_type_in_intersection() {
    // Constructor type in intersection needs parentheses
    let output = emit_dts("export type T = (new (x: string) => object) & { tag: string };");
    println!("ctor in intersection:\n{output}");
    assert!(
        output.contains("(new (x: string) => object) & {"),
        "constructor type in intersection should be parenthesized: {output}"
    );
}

#[test]
fn explore_type_operator_in_array() {
    // `keyof T` in array should get parenthesized: `(keyof T)[]`
    let output = emit_dts("export type T<U> = (keyof U)[];");
    println!("type op in array:\n{output}");
    assert!(
        output.contains("(keyof U)[]"),
        "type operator in array should be parenthesized: {output}"
    );
}

#[test]
fn explore_conditional_type_in_array() {
    // (T extends U ? X : Y)[] - conditional type in array needs parens
    let output = emit_dts("export type T<U> = (U extends string ? 'yes' : 'no')[];");
    println!("conditional in array:\n{output}");
    assert!(
        output.contains("(U extends string"),
        "conditional in array should be parenthesized: {output}"
    );
    assert!(
        output.contains("[]"),
        "array brackets should be present: {output}"
    );
}

#[test]
fn explore_intersection_type_in_union() {
    // Intersection types inside unions don't need parens (& binds tighter)
    let output = emit_dts("export type T = A & B | C & D;");
    println!("intersection in union:\n{output}");
    // No parens needed since & binds tighter
    assert!(
        output.contains("A & B | C & D"),
        "intersection in union should not need parens: {output}"
    );
}

#[test]
fn explore_function_type_in_conditional_extends() {
    // Function type in conditional extends position might need parens
    let output = emit_dts("export type T<F> = F extends (() => infer R) ? R : never;");
    println!("fn in conditional:\n{output}");
    assert!(
        output.contains("infer R"),
        "infer R should be present: {output}"
    );
}

#[test]
fn explore_complex_nested_types() {
    // Deeply nested type with multiple operators
    let output = emit_dts(
        "export type T = {
    readonly [K in keyof any as `on${string & K}`]: ((event: K) => void) | null;
};",
    );
    println!("complex nested:\n{output}");
    assert!(
        output.contains("readonly [K in keyof any as `on${string & K}`]"),
        "mapped type with as clause should be preserved: {output}"
    );
}

#[test]
fn explore_declare_var_vs_let_vs_const() {
    // declare var/let/const all have specific behavior in .d.ts
    let output = emit_dts(
        "export declare var a: string;
export declare let b: number;
export declare const c: boolean;",
    );
    println!("var/let/const:\n{output}");
    assert!(
        output.contains("export declare var a: string;"),
        "var should be preserved: {output}"
    );
    assert!(
        output.contains("export declare let b: number;"),
        "let should be preserved: {output}"
    );
    assert!(
        output.contains("export declare const c: boolean;"),
        "const should be preserved: {output}"
    );
}

#[test]
fn explore_export_type_star() {
    let output = emit_dts("export type * from './module';");
    println!("export type *:\n{output}");
    assert!(
        output.contains("export type * from"),
        "export type * should be preserved: {output}"
    );
}

#[test]
fn explore_export_type_star_as_ns() {
    let output = emit_dts("export type * as ns from './module';");
    println!("export type * as ns:\n{output}");
    assert!(
        output.contains("export type * as ns from"),
        "export type * as ns should be preserved: {output}"
    );
}

#[test]
fn explore_class_with_declare_property() {
    // `declare` keyword on class property — tsc strips this in .d.ts
    let output = emit_dts(
        "export class Foo {
    declare bar: string;
}",
    );
    println!("declare prop:\n{output}");
    // tsc strips `declare` from class members in .d.ts
    assert!(
        output.contains("bar: string;"),
        "bar should be present: {output}"
    );
    let bar_line = output.lines().find(|l| l.contains("bar: string")).unwrap();
    assert!(
        !bar_line.contains("declare"),
        "declare should be stripped from class member: {output}"
    );
}

#[test]
fn explore_async_generator() {
    let output = emit_dts("export async function* gen(): AsyncGenerator<number> { yield 1; }");
    println!("async generator:\n{output}");
    // tsc strips async and * from .d.ts
    assert!(
        !output.contains("async"),
        "async should be stripped: {output}"
    );
    assert!(!output.contains("*"), "* should be stripped: {output}");
    assert!(
        output.contains("gen(): AsyncGenerator<number>;"),
        "return type should be preserved: {output}"
    );
}

#[test]
fn explore_type_predicate_this() {
    // `this is Type` predicate
    let output = emit_dts(
        "export class Animal {
    isFlying(): this is FlyingAnimal { return false; }
}
interface FlyingAnimal extends Animal { fly(): void; }",
    );
    println!("this predicate:\n{output}");
    assert!(
        output.contains("this is FlyingAnimal"),
        "this type predicate should be preserved: {output}"
    );
}

#[test]
fn explore_constructor_type_with_generics() {
    let output = emit_dts("export type T = new <U>(x: U) => U;");
    println!("ctor generic:\n{output}");
    assert!(
        output.contains("new <U>(x: U) => U"),
        "generic constructor type should be preserved: {output}"
    );
}

#[test]
fn explore_nested_generics_in_function_type() {
    let output =
        emit_dts("export type T = <A, B extends Record<string, A>>(x: A, y: B) => Map<A, B>;");
    println!("nested generics:\n{output}");
    assert!(
        output.contains("<A, B extends Record<string, A>>"),
        "nested generics should be preserved: {output}"
    );
    assert!(
        output.contains("Map<A, B>"),
        "return type should be preserved: {output}"
    );
}

#[test]
fn explore_declare_property_with_modifiers() {
    // declare property with access modifiers — tsc strips `declare` from class members
    let output = emit_dts(
        "export class Foo {
    declare protected bar: string;
    declare static baz: number;
}",
    );
    println!("declare prop modifiers:\n{output}");
    assert!(
        output.contains("protected bar: string;"),
        "protected bar should be present: {output}"
    );
    assert!(
        output.contains("static baz: number;"),
        "static baz should be present: {output}"
    );
    // `declare` should be stripped from members
    for line in output.lines() {
        if line.contains("bar:") || line.contains("baz:") {
            assert!(
                !line.contains("declare"),
                "declare should be stripped from member: {line}"
            );
        }
    }
}

#[test]
fn explore_class_with_multiple_declare_properties() {
    // tsc strips `declare` from class members in .d.ts
    let output = emit_dts(
        "export class Foo {
    declare x: string;
    y: number;
    declare z: boolean;
}",
    );
    println!("multiple declare props:\n{output}");
    assert!(
        output.contains("x: string;"),
        "x should be present: {output}"
    );
    assert!(
        output.contains("y: number;"),
        "y should be present: {output}"
    );
    assert!(
        output.contains("z: boolean;"),
        "z should be present: {output}"
    );
}

#[test]
fn explore_abstract_accessor_declaration() {
    // abstract get/set accessors
    let output = emit_dts(
        "export abstract class Foo {
    abstract get name(): string;
    abstract set name(val: string);
}",
    );
    println!("abstract accessors:\n{output}");
    assert!(
        output.contains("abstract get name(): string;"),
        "abstract getter should be preserved: {output}"
    );
    assert!(
        output.contains("abstract set name(val: string);"),
        "abstract setter should be preserved: {output}"
    );
}

#[test]
fn explore_constructor_overloads_with_accessibility() {
    let output = emit_dts(
        "export class Foo {
    private constructor(x: string);
    private constructor(x: number);
    private constructor(x: any) {}
}",
    );
    println!("ctor overloads with access:\n{output}");
    let ctor_count = output.matches("private constructor(").count();
    assert_eq!(
        ctor_count, 2,
        "Should have 2 private constructor overloads (not implementation): {output}"
    );
}

#[test]
fn explore_generic_method_with_constraint() {
    let output = emit_dts(
        "export class Container {
    get<T extends object>(key: string): T { return {} as T; }
}",
    );
    println!("generic method constraint:\n{output}");
    assert!(
        output.contains("get<T extends object>(key: string): T;"),
        "generic method with constraint should be preserved: {output}"
    );
}

#[test]
fn explore_index_signature_with_readonly() {
    let output = emit_dts(
        "export interface Dict {
    readonly [key: string]: number;
}",
    );
    println!("readonly index sig:\n{output}");
    assert!(
        output.contains("readonly [key: string]: number;"),
        "readonly index signature should be preserved: {output}"
    );
}

#[test]
fn explore_computed_property_with_well_known_symbol() {
    let output = emit_dts(
        "export class MyIterable {
    [Symbol.iterator](): Iterator<number> { return [].values(); }
}",
    );
    println!("well-known symbol:\n{output}");
    assert!(
        output.contains("[Symbol.iterator]()"),
        "well-known symbol should be preserved: {output}"
    );
}

#[test]
fn explore_nested_mapped_type_with_template_keys() {
    let output = emit_dts(
        "export type EventHandlers<T> = {
    [K in keyof T as K extends string ? `on${Capitalize<K>}` : never]: (event: T[K]) => void;
};",
    );
    println!("nested mapped template:\n{output}");
    assert!(
        output.contains("as K extends string ? `on${Capitalize<K>}` : never"),
        "mapped type with template key should be preserved: {output}"
    );
}

#[test]
fn explore_class_with_abstract_accessor_and_regular() {
    let output = emit_dts(
        "export abstract class Base {
    abstract get id(): string;
    get name(): string { return ''; }
}",
    );
    println!("abstract + regular accessors:\n{output}");
    assert!(
        output.contains("abstract get id(): string;"),
        "abstract accessor should be preserved: {output}"
    );
    assert!(
        output.contains("get name(): string;"),
        "regular accessor should be preserved: {output}"
    );
}

#[test]
fn explore_function_with_this_type_return() {
    let output = emit_dts(
        "export declare class Builder {
    withName(name: string): this;
    build(): object;
}",
    );
    println!("this return type:\n{output}");
    let expected = "export declare class Builder {\n    withName(name: string): this;\n    build(): object;\n}\n";
    assert_eq!(output, expected, "Mismatch");
}

// =============================================================================
// Round 6 - More targeted edge case testing
// =============================================================================

#[test]
fn explore_assertion_signature_in_class() {
    let output = emit_dts(
        "export class Guard {
    assertValid(value: unknown): asserts value is string {
        if (typeof value !== 'string') throw new Error();
    }
}",
    );
    println!("assertion in class:\n{output}");
    assert!(
        output.contains("assertValid(value: unknown): asserts value is string;"),
        "assertion signature should be preserved: {output}"
    );
}

#[test]
fn explore_class_method_return_type_with_function_type() {
    // Method returning a function type
    let output = emit_dts(
        "export declare class Foo {
    getHandler(): (event: string) => void;
}",
    );
    println!("method returning fn type:\n{output}");
    assert!(
        output.contains("getHandler(): (event: string) => void;"),
        "function return type should be preserved: {output}"
    );
}

#[test]
fn explore_interface_with_string_index_and_numeric_index() {
    let output = emit_dts(
        "export interface Mixed {
    [key: string]: any;
    [index: number]: string;
    length: number;
}",
    );
    println!("mixed index sigs:\n{output}");
    assert!(
        output.contains("[key: string]: any;"),
        "string index should be present: {output}"
    );
    assert!(
        output.contains("[index: number]: string;"),
        "numeric index should be present: {output}"
    );
    assert!(
        output.contains("length: number;"),
        "length should be present: {output}"
    );
}

#[test]
fn explore_readonly_tuple_with_labels() {
    let output = emit_dts("export type Point3D = readonly [x: number, y: number, z: number];");
    println!("readonly labeled tuple:\n{output}");
    assert!(
        output.contains("readonly [x: number, y: number, z: number]"),
        "readonly labeled tuple should be preserved: {output}"
    );
}

#[test]
fn explore_method_with_overloaded_generics() {
    let output = emit_dts(
        "export interface Repository {
    find<T extends object>(id: string): T;
    find<T extends object>(query: Partial<T>): T[];
}",
    );
    println!("overloaded generics:\n{output}");
    assert!(
        output.contains("find<T extends object>(id: string): T;"),
        "overload 1 should be present: {output}"
    );
    assert!(
        output.contains("find<T extends object>(query: Partial<T>): T[];"),
        "overload 2 should be present: {output}"
    );
}

#[test]
fn explore_export_default_expression_identifier() {
    // export default someVar
    let output = emit_dts(
        "declare const x: number;
export default x;",
    );
    println!("export default identifier:\n{output}");
    assert!(
        output.contains("export default x;"),
        "export default identifier should be preserved: {output}"
    );
}

#[test]
fn explore_class_with_index_and_computed_symbol() {
    // Class with both index signature and computed symbol property
    let output = emit_dts(
        "export class Dict {
    [key: string]: any;
    [Symbol.toPrimitive](): string { return ''; }
}",
    );
    println!("index + symbol:\n{output}");
    assert!(
        output.contains("[key: string]: any;"),
        "index sig should be present: {output}"
    );
    assert!(
        output.contains("[Symbol.toPrimitive]()"),
        "symbol method should be present: {output}"
    );
}

#[test]
fn explore_multiple_export_as() {
    let output = emit_dts("export { default as React } from 'react';");
    println!("export as:\n{output}");
    assert!(
        output.contains("default as React"),
        "export as should be preserved: {output}"
    );
}

#[test]
fn explore_class_with_definite_and_optional_properties() {
    let output = emit_dts(
        "export class Foo {
    bar?: string;
    baz!: number;
    qux: boolean;
}",
    );
    println!("definite + optional:\n{output}");
    assert!(
        output.contains("bar?: string;"),
        "optional prop should be preserved: {output}"
    );
    // tsc strips ! in .d.ts
    assert!(
        output.contains("baz: number;"),
        "definite assignment should be stripped: {output}"
    );
    assert!(!output.contains("baz!:"), "! should not appear: {output}");
    assert!(
        output.contains("qux: boolean;"),
        "normal prop should be preserved: {output}"
    );
}

#[test]
fn explore_class_with_only_static_block() {
    // A class that has only a static block should emit an empty class body
    let output = emit_dts(
        "export class Init {
    static {
        console.log('init');
    }
}",
    );
    println!("only static block:\n{output}");
    assert!(
        !output.contains("static {"),
        "static block should be stripped: {output}"
    );
    // Class should still emit, even with empty body
    assert!(
        output.contains("export declare class Init"),
        "class should still be emitted: {output}"
    );
}

#[test]
fn explore_template_literal_with_multiple_spans() {
    let output = emit_dts("export type EventKey = `${string}_${number}_${boolean}`;");
    println!("multi-span template:\n{output}");
    assert!(
        output.contains("`${string}_${number}_${boolean}`"),
        "multi-span template should be preserved: {output}"
    );
}

#[test]
fn explore_conditional_type_distributive_constraint() {
    let output = emit_dts("export type Exclude<T, U> = T extends U ? never : T;");
    println!("exclude type:\n{output}");
    let expected = "export type Exclude<T, U> = T extends U ? never : T;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn explore_keyof_typeof_combined() {
    let output = emit_dts(
        "declare const obj: { a: 1; b: 2; };
export type Keys = keyof typeof obj;",
    );
    println!("keyof typeof:\n{output}");
    assert!(
        output.contains("keyof typeof obj"),
        "keyof typeof should be preserved: {output}"
    );
}

#[test]
fn explore_class_protected_static_abstract() {
    let output = emit_dts(
        "export abstract class Base {
    protected static abstract create(): Base;
}",
    );
    println!("protected static abstract:\n{output}");
    // Order in tsc: protected static abstract or protected abstract static
    assert!(
        output.contains("protected")
            && output.contains("static")
            && output.contains("abstract")
            && output.contains("create()"),
        "all modifiers should be present: {output}"
    );
}

#[test]
fn explore_function_type_with_rest_and_optional() {
    let output = emit_dts("export type Fn = (a: string, b?: number, ...rest: boolean[]) => void;");
    println!("fn type rest+opt:\n{output}");
    assert!(
        output.contains("(a: string, b?: number, ...rest: boolean[]) => void"),
        "function type params should be preserved: {output}"
    );
}

#[test]
fn explore_type_predicate_in_type_literal() {
    let output = emit_dts(
        "export type TypeGuards = {
    isString(value: unknown): value is string;
    isNumber(value: unknown): value is number;
};",
    );
    println!("type pred in literal:\n{output}");
    assert!(
        output.contains("isString(value: unknown): value is string;"),
        "isString predicate should be preserved: {output}"
    );
    assert!(
        output.contains("isNumber(value: unknown): value is number;"),
        "isNumber predicate should be preserved: {output}"
    );
}

// =============================================================================
// Round 7 - Exact comparison with tsc output
// =============================================================================

#[test]
fn exact_tsc_assertion_function_simple() {
    // tsc output: export declare function assert(val: unknown): asserts val;
    let output = emit_dts("export declare function assert(val: unknown): asserts val;");
    println!("assertion fn simple:\n{output}");
    let expected = "export declare function assert(val: unknown): asserts val;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_assertion_function_with_type() {
    // tsc output: export declare function assertStr(val: unknown): asserts val is string;
    let output =
        emit_dts("export declare function assertStr(val: unknown): asserts val is string;");
    println!("assertion fn typed:\n{output}");
    let expected = "export declare function assertStr(val: unknown): asserts val is string;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_readonly_tuple() {
    let output = emit_dts("export type T1 = readonly [string, number];");
    println!("readonly tuple:\n{output}");
    let expected = "export type T1 = readonly [string, number];\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_named_tuple_with_rest() {
    let output = emit_dts("export type T2 = [first: string, ...rest: number[]];");
    println!("named tuple rest:\n{output}");
    let expected = "export type T2 = [first: string, ...rest: number[]];\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_template_literal_union() {
    let output = emit_dts("export type T4 = `${'a' | 'b'}-${'x' | 'y'}`;");
    println!("template union:\n{output}");
    // tsc outputs: export type T4 = `${'a' | 'b'}-${'x' | 'y'}`;
    // or with double quotes: `${"a" | "b"}-${"x" | "y"}`
    assert!(
        output.contains("`${'a' | 'b'}-${'x' | 'y'}`")
            || output.contains("`${\"a\" | \"b\"}-${\"x\" | \"y\"}`"),
        "template literal union should be preserved: {output}"
    );
}

#[test]
fn exact_tsc_mapped_type_with_template_key() {
    let output = emit_dts(
        "export type T5<T> = { [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K] };",
    );
    println!("mapped template key:\n{output}");
    // tsc output (multi-line):
    // export type T5<T> = {
    //     [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K];
    // };
    let expected = "export type T5<T> = {\n    [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K];\n};\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_construct_signatures() {
    let output = emit_dts(
        "export interface I2 {
    new (x: string): object;
    new (x: number): object;
}",
    );
    println!("construct sigs:\n{output}");
    let expected =
        "export interface I2 {\n    new (x: string): object;\n    new (x: number): object;\n}\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_exclude_type() {
    let output = emit_dts("export type Exclude<T, U> = T extends U ? never : T;");
    println!("exclude:\n{output}");
    let expected = "export type Exclude<T, U> = T extends U ? never : T;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_extract_type() {
    let output = emit_dts("export type Extract<T, U> = T extends U ? T : never;");
    println!("extract:\n{output}");
    let expected = "export type Extract<T, U> = T extends U ? T : never;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_nonnullable() {
    let output = emit_dts("export type NonNullable<T> = T & {};");
    println!("nonnullable:\n{output}");
    let expected = "export type NonNullable<T> = T & {};\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_returntype() {
    let output = emit_dts(
        "export type ReturnType<T extends (...args: any) => any> = T extends (...args: any) => infer R ? R : any;",
    );
    println!("returntype:\n{output}");
    let expected = "export type ReturnType<T extends (...args: any) => any> = T extends (...args: any) => infer R ? R : any;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_class_with_static_block() {
    // tsc strips static blocks entirely
    let output = emit_dts(
        "export class Foo {
    static x: number;
    static {
        this.x = 42;
    }
    bar(): void {}
}",
    );
    println!("class static block:\n{output}");
    let expected = "export declare class Foo {\n    static x: number;\n    bar(): void;\n}\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_this_parameter() {
    let output =
        emit_dts("export declare function handler(this: HTMLElement, event: Event): void;");
    println!("this param:\n{output}");
    let expected = "export declare function handler(this: HTMLElement, event: Event): void;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_static_accessor() {
    let output = emit_dts(
        "export class Foo {
    static accessor bar: string = '';
}",
    );
    println!("static accessor:\n{output}");
    let expected = "export declare class Foo {\n    static accessor bar: string;\n}\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_const_type_parameter() {
    let output = emit_dts("export declare function identity<const T>(value: T): T;");
    println!("const type param:\n{output}");
    let expected = "export declare function identity<const T>(value: T): T;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_multiple_variable_declarators() {
    let output = emit_dts("export declare const a: string, b: number;");
    println!("multi declarators:\n{output}");
    let expected = "export declare const a: string, b: number;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_generic_call_signatures_in_interface() {
    let output = emit_dts(
        "export interface Converter {
    <T extends string>(input: T): number;
    <T extends number>(input: T): string;
}",
    );
    println!("generic call sigs:\n{output}");
    let expected = "export interface Converter {\n    <T extends string>(input: T): number;\n    <T extends number>(input: T): string;\n}\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_recursive_type() {
    let output = emit_dts(
        "export type LinkedList<T> = {
    value: T;
    next: LinkedList<T> | null;
};",
    );
    println!("recursive type:\n{output}");
    let expected =
        "export type LinkedList<T> = {\n    value: T;\n    next: LinkedList<T> | null;\n};\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_unwrap_promise() {
    let output = emit_dts(
        "export type UnwrapPromise<T> = T extends Promise<infer U> ? UnwrapPromise<U> : T;",
    );
    println!("unwrap promise:\n{output}");
    let expected =
        "export type UnwrapPromise<T> = T extends Promise<infer U> ? UnwrapPromise<U> : T;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_complex_mapped_merge() {
    let output = emit_dts(
        "export type Merge<A, B> = {
    [K in keyof A | keyof B]: K extends keyof A & keyof B ? A[K] | B[K] : K extends keyof A ? A[K] : K extends keyof B ? B[K] : never;
};",
    );
    println!("merge type:\n{output}");
    let expected = "export type Merge<A, B> = {\n    [K in keyof A | keyof B]: K extends keyof A & keyof B ? A[K] | B[K] : K extends keyof A ? A[K] : K extends keyof B ? B[K] : never;\n};\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_abstract_constructor_type_alias() {
    let output =
        emit_dts("export type AbstractConstructor = abstract new (...args: any[]) => any;");
    println!("abstract ctor type:\n{output}");
    let expected = "export type AbstractConstructor = abstract new (...args: any[]) => any;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_asserts_is_never() {
    // tsc preserves `asserts x is never` — `never` is a valid type predicate target
    let output = emit_dts("export declare function assertNever(x: never): asserts x is never;");
    println!("asserts never:\n{output}");
    let expected = "export declare function assertNever(x: never): asserts x is never;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_asserts_is_unknown() {
    // tsc preserves `asserts x is unknown` — `unknown` is a valid type predicate target
    let output = emit_dts("export declare function assertUnknown(x: any): asserts x is unknown;");
    println!("asserts unknown:\n{output}");
    let expected = "export declare function assertUnknown(x: any): asserts x is unknown;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_type_guard_is_never() {
    // `x is never` type guard should also be preserved
    let output = emit_dts("export declare function isNever(x: unknown): x is never;");
    println!("guard never:\n{output}");
    let expected = "export declare function isNever(x: unknown): x is never;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_type_guard_is_unknown() {
    let output = emit_dts("export declare function isUnknown(x: any): x is unknown;");
    println!("guard unknown:\n{output}");
    let expected = "export declare function isUnknown(x: any): x is unknown;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_simple_asserts_no_type() {
    // Simple `asserts x` without `is Type` should NOT emit `is` part
    let output = emit_dts("export declare function assertSimple(x: unknown): asserts x;");
    println!("simple asserts:\n{output}");
    let expected = "export declare function assertSimple(x: unknown): asserts x;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_asserts_is_void() {
    // `asserts x is void` should be preserved
    let output = emit_dts("export declare function assertVoid(x: unknown): asserts x is void;");
    println!("asserts void:\n{output}");
    let expected = "export declare function assertVoid(x: unknown): asserts x is void;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_class_with_instance_and_static_accessor() {
    let output = emit_dts(
        "export class C {
    accessor x: number = 0;
    static accessor y: string = '';
}",
    );
    println!("accessors:\n{output}");
    let expected =
        "export declare class C {\n    accessor x: number;\n    static accessor y: string;\n}\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn fix_identity_call_mutual_recursion_does_not_hang() {
    // Regression: const_literal_identity_call_text used to create a fresh
    // RecursionGuard on each call, so a mutually-recursive pair like
    //   const a = id(b); const b = id(a);
    // would loop forever.  The guard is now threaded through, so the cycle is
    // detected and both consts fall back to their inferred types.
    let source = r#"
function id<T>(x: T): T { return x; }
export const a = id(b);
export const b = id(a);
"#;
    // Must complete without hanging; output type is not the goal of this test.
    let output = emit_dts(source);
    assert!(
        !output.is_empty(),
        "emitter should produce output: {output}"
    );
}
