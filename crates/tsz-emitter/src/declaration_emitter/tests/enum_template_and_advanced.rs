use super::*;

// =====================================================================
// Template literal enum evaluation
// =====================================================================

#[test]
fn test_enum_template_literal_value_vs_tsc() {
    // tsc evaluates template literals in enum members: `${E.A}_world` -> "hello_world"
    let result = emit_dts("export enum E {\n    A = \"hello\",\n    B = `${E.A}_world`,\n}\n");
    assert!(
        result.contains("B = \"hello_world\""),
        "Should evaluate template literal enum value: {result}"
    );
}

#[test]
fn test_enum_template_literal_chained_vs_tsc() {
    // Multiple levels of template literal evaluation in enums
    let result = emit_dts(
        r#"export enum Actions {
    Click = "click",
    Hover = "hover",
    OnClick = `on_${Actions.Click}`,
    OnHover = `on_${Actions.Hover}`,
    Nested = `prefix_${Actions.OnClick}_suffix`,
}
"#,
    );
    let expected = r#"export declare enum Actions {
    Click = "click",
    Hover = "hover",
    OnClick = "on_click",
    OnHover = "on_hover",
    Nested = "prefix_on_click_suffix"
}
"#;
    assert_eq!(
        result, expected,
        "Template literal chained enum values should match tsc"
    );
}

#[test]
fn test_enum_template_literal_multiple_spans_vs_tsc() {
    // Template literal with multiple substitutions
    let result = emit_dts(
        r#"export enum E {
    A = "x",
    B = "y",
    C = `${E.A}_${E.B}_z`,
}
"#,
    );
    assert!(
        result.contains(r#"C = "x_y_z""#),
        "Should evaluate multi-span template: {result}"
    );
}

#[test]
fn test_enum_no_substitution_template_vs_tsc() {
    // No-substitution template backtick literal should evaluate to string
    let result = emit_dts("export enum E {\n    A = `hello`,\n}\n");
    assert!(
        result.contains("A = \"hello\""),
        "No-sub template should produce string: {result}"
    );
}

#[test]
fn test_probe_string_enum_concat() {
    let result = emit_dts(
        r#"export enum S {
    Prefix = "PRE",
    Full = Prefix + "_SUFFIX",
}
"#,
    );
    // tsc evaluates: Full = "PRE_SUFFIX"
    assert!(
        result.contains(r#"Full = "PRE_SUFFIX""#),
        "Should evaluate string concat: {result}"
    );
}

// ==========================================================================
// Edge case probes - Round 14
// ==========================================================================

#[test]
fn probe_type_literal_with_call_and_construct_signatures() {
    // Type literal with call signatures, construct signatures, and properties
    let output = emit_dts(
        r#"export type Complex = {
    (x: string): number;
    new (y: boolean): object;
    name: string;
};"#,
    );
    assert!(
        output.contains("(x: string): number;"),
        "call sig: {output}"
    );
    assert!(
        output.contains("new (y: boolean): object;"),
        "construct sig: {output}"
    );
    assert!(output.contains("name: string;"), "property: {output}");
}

#[test]
fn probe_const_type_parameter() {
    // TS 5.0 `const` type parameter modifier
    let output =
        emit_dts("export declare function foo<const T extends readonly unknown[]>(args: T): T;");
    assert!(
        output.contains("const T"),
        "const type param modifier should be preserved: {output}"
    );
}

#[test]
fn probe_variance_annotations() {
    // TS 4.7 variance annotations (in/out)
    let output = emit_dts(
        r#"export interface Getter<out T> {
    get(): T;
}
export interface Setter<in T> {
    set(value: T): void;
}
export interface State<in out T> {
    get(): T;
    set(value: T): void;
}"#,
    );
    assert!(output.contains("out T"), "out variance: {output}");
    assert!(output.contains("in T"), "in variance: {output}");
    assert!(output.contains("in out T"), "in out variance: {output}");
}

#[test]
fn probe_abstract_construct_signature_in_interface() {
    // Abstract construct signature in type
    let output = emit_dts("export type AbstractCtor = abstract new <T>() => T;");
    assert!(
        output.contains("abstract new"),
        "abstract constructor type: {output}"
    );
}

#[test]
fn probe_nested_mapped_type_with_as_clause() {
    // Mapped type with `as` clause using template literal
    let output = emit_dts(
        r#"export type Getters<T> = {
    [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K];
};"#,
    );
    assert!(
        output.contains("as `get${Capitalize"),
        "as clause with template: {output}"
    );
    assert!(output.contains("() => T[K]"), "return type: {output}");
}

#[test]
fn probe_class_static_block_omitted() {
    // Static blocks should be omitted in DTS
    let output = emit_dts(
        r#"export class Foo {
    static x: number;
    static {
        Foo.x = 42;
    }
}"#,
    );
    assert!(
        !output.contains("static {"),
        "static block should be omitted: {output}"
    );
    assert!(
        output.contains("static x: number;"),
        "static prop should remain: {output}"
    );
}

#[test]
fn probe_conditional_type_nested_in_extends() {
    // Nested conditional types - right-associativity
    let output = emit_dts(
        "export type Deep<T> = T extends string ? 1 : T extends number ? 2 : T extends boolean ? 3 : 4;",
    );
    assert!(
        output.contains("T extends string ? 1 : T extends number ? 2 : T extends boolean ? 3 : 4"),
        "nested conditional: {output}"
    );
}

#[test]
fn probe_type_operator_on_union_needs_parens() {
    // `keyof (A | B)` needs parens around the union
    let output = emit_dts("export type K = keyof (string | number);");
    // tsc emits: type K = keyof (string | number);
    assert!(
        output.contains("keyof (string | number)"),
        "keyof union needs parens: {output}"
    );
}

#[test]
fn probe_readonly_tuple_type() {
    // readonly tuple
    let output = emit_dts("export type RT = readonly [string, number, ...boolean[]];");
    assert!(
        output.contains("readonly [string, number, ...boolean[]]"),
        "readonly tuple: {output}"
    );
}

#[test]
fn probe_rest_type_in_tuple() {
    // Rest type in tuple
    let output = emit_dts("export type Spread = [string, ...number[], boolean];");
    assert!(
        output.contains("[string, ...number[], boolean]"),
        "rest in middle: {output}"
    );
}

#[test]
fn probe_optional_type_in_tuple() {
    // Optional element in tuple
    let output = emit_dts("export type Opt = [string, number?, boolean?];");
    assert!(
        output.contains("[string, number?, boolean?]"),
        "optional tuple elements: {output}"
    );
}

#[test]
fn probe_intersection_with_union_needs_parens() {
    // Union inside intersection needs parens
    let output = emit_dts("export type X = (string | number) & { tag: true };");
    assert!(
        output.contains("(string | number) &"),
        "union in intersection needs parens: {output}"
    );
}

#[test]
fn probe_function_type_in_union_needs_parens() {
    // Function type in union needs parens
    let output = emit_dts("export type F = ((x: number) => string) | null;");
    assert!(
        output.contains("((x: number) => string) | null"),
        "function type in union needs parens: {output}"
    );
}

#[test]
fn probe_class_with_override_modifier_stripped() {
    // tsc strips `override` in .d.ts output
    let output = emit_dts(
        r#"declare class Base {
    method(): void;
}
export declare class Child extends Base {
    override method(): void;
}"#,
    );
    assert!(
        !output.contains("override"),
        "override modifier should be stripped in .d.ts: {output}"
    );
}

#[test]
fn probe_export_type_star_from() {
    // export type * from should be preserved
    let output = emit_dts(r#"export type * from "./types";"#);
    assert!(
        output.contains("export type *"),
        "export type * should be preserved: {output}"
    );
}

#[test]
fn probe_export_type_star_as_ns() {
    // export type * as ns from should be preserved
    let output = emit_dts(r#"export type * as ns from "./types";"#);
    assert!(
        output.contains("export type * as ns"),
        "export type * as ns should be preserved: {output}"
    );
}

#[test]
fn probe_await_using_declaration() {
    // `await using` declarations should emit as `const` in .d.ts
    let output = emit_dts("export await using x: AsyncDisposable = getResource();");
    assert!(
        output.contains("export declare const x: AsyncDisposable;"),
        "await using should emit as const: {output}"
    );
}

#[test]
fn probe_class_accessor_modifier() {
    // `accessor` field declaration (TC39 auto-accessors)
    let output = emit_dts(
        r#"export class Foo {
    accessor name: string = "default";
}"#,
    );
    assert!(
        output.contains("accessor name: string;"),
        "accessor keyword should be preserved: {output}"
    );
}

#[test]
fn probe_negative_numeric_literal_type() {
    // Negative numeric literal in type position
    let output = emit_dts("export type Neg = -1 | -2 | -3;");
    assert!(
        output.contains("-1 | -2 | -3"),
        "negative literals: {output}"
    );
}

#[test]
fn probe_bigint_literal_type_in_union() {
    // BigInt literal type in union
    let output = emit_dts("export type Big = 100n | 200n;");
    assert!(output.contains("100n"), "bigint literal type: {output}");
    assert!(output.contains("200n"), "bigint literal type: {output}");
}

#[test]
fn probe_constructor_overloads_in_class() {
    // Multiple constructor overloads - only signatures, not implementation
    let output = emit_dts(
        r#"export class Multi {
    constructor(x: string);
    constructor(x: number);
    constructor(x: string | number) {}
}"#,
    );
    assert!(
        output.contains("constructor(x: string);"),
        "first overload: {output}"
    );
    assert!(
        output.contains("constructor(x: number);"),
        "second overload: {output}"
    );
    // Implementation should be stripped
    assert!(
        !output.contains("string | number"),
        "implementation should be stripped: {output}"
    );
}

#[test]
fn probe_declare_global_in_module() {
    // `declare global` in a module should be preserved
    let output = emit_dts(
        r#"export {};
declare global {
    interface Window {
        myProp: string;
    }
}"#,
    );
    assert!(
        output.contains("declare global"),
        "declare global should be preserved: {output}"
    );
    assert!(
        output.contains("myProp: string"),
        "global interface member should be emitted: {output}"
    );
}

#[test]
fn probe_type_alias_recursive() {
    // Recursive type alias (JSON type)
    let output = emit_dts(
        r#"export type Json = string | number | boolean | null | Json[] | { [key: string]: Json };"#,
    );
    assert!(output.contains("Json[]"), "recursive array: {output}");
    assert!(
        output.contains("[key: string]: Json"),
        "recursive index sig: {output}"
    );
}

#[test]
fn probe_export_as_namespace() {
    // UMD global export
    let output = emit_dts(r#"export as namespace myLib;"#);
    assert!(
        output.contains("export as namespace myLib;"),
        "export as namespace should be preserved: {output}"
    );
}

#[test]
fn probe_module_declaration_string_name() {
    // Module with string name (ambient module)
    let output = emit_dts(
        r#"declare module "my-module" {
    export function hello(): void;
}"#,
    );
    assert!(
        output.contains(r#"declare module "my-module""#),
        "ambient module with string name: {output}"
    );
    assert!(
        output.contains("function hello(): void;"),
        "module member: {output}"
    );
}

#[test]
fn probe_class_with_declare_field() {
    // `declare` fields in class should have declare stripped in .d.ts
    let output = emit_dts(
        r#"export class Foo {
    declare x: string;
}"#,
    );
    // tsc strips `declare` from class fields in .d.ts
    assert!(
        output.contains("x: string;"),
        "declare field should emit as regular field: {output}"
    );
    // The `declare` keyword should be stripped from the field
    assert!(
        !output.contains("declare x:"),
        "declare should be stripped from class field: {output}"
    );
}

#[test]
fn probe_export_default_abstract_class_with_method() {
    // export default abstract class with abstract methods
    let output = emit_dts(
        r#"export default abstract class Foo {
    abstract bar(): void;
    abstract baz(x: string): number;
}"#,
    );
    assert!(
        output.contains("abstract class"),
        "abstract class should be emitted: {output}"
    );
    assert!(
        output.contains("abstract bar(): void;"),
        "abstract method should be emitted: {output}"
    );
    assert!(
        output.contains("abstract baz(x: string): number;"),
        "abstract method with param: {output}"
    );
}

#[test]
fn probe_interface_with_readonly_index_signature() {
    // readonly index signature in interface
    let output = emit_dts(
        r#"export interface Dict {
    readonly [key: string]: number;
}"#,
    );
    assert!(
        output.contains("readonly [key: string]: number;"),
        "readonly index signature: {output}"
    );
}

#[test]
fn probe_type_literal_with_optional_method() {
    // Type literal with optional method signature
    let output = emit_dts(
        r#"export type Handler = {
    onSuccess?(data: string): void;
    onError?(error: Error): void;
};"#,
    );
    assert!(
        output.contains("onSuccess?(data: string): void;"),
        "optional method in type literal: {output}"
    );
    assert!(
        output.contains("onError?(error: Error): void;"),
        "optional method in type literal: {output}"
    );
}

#[test]
fn probe_enum_with_non_identifier_member_name() {
    // Enum member with string literal name
    let output = emit_dts(
        r#"export enum E {
    "hello world" = 1,
    "foo-bar" = 2,
}"#,
    );
    // tsc emits these as: ["hello world"] = 1
    assert!(
        output.contains(r#""hello world""#) || output.contains(r#"["hello world"]"#),
        "string literal enum member name: {output}"
    );
}

#[test]
fn probe_conditional_type_with_infer_constraint() {
    // infer with extends constraint (TS 4.7)
    let output =
        emit_dts("export type ElementType<T> = T extends (infer U extends string)[] ? U : never;");
    assert!(
        output.contains("infer U extends string"),
        "infer with extends constraint: {output}"
    );
}

#[test]
fn probe_array_of_function_type_needs_parens() {
    // Array of function type needs parens: ((x: number) => string)[]
    let output = emit_dts("export type FnArr = ((x: number) => string)[];");
    assert!(
        output.contains("((x: number) => string)[]"),
        "function type in array needs parens: {output}"
    );
}

#[test]
fn probe_array_of_union_type_needs_parens() {
    // Array of union type needs parens: (string | number)[]
    let output = emit_dts("export type UnionArr = (string | number)[];");
    assert!(
        output.contains("(string | number)[]"),
        "union in array needs parens: {output}"
    );
}

#[test]
fn probe_indexed_access_on_union_needs_parens() {
    // Indexed access on union type needs parens: (A | B)["key"]
    let output = emit_dts(r#"export type X = (string[] | number[])["length"];"#);
    assert!(
        output.contains(r#"(string[] | number[])["length"]"#),
        "indexed access on union needs parens: {output}"
    );
}

#[test]
fn probe_export_default_function_with_type_params() {
    // export default function with type parameters
    let output = emit_dts("export default function identity<T>(x: T): T { return x; }");
    assert!(
        output.contains("function identity<T>(x: T): T;"),
        "default generic function: {output}"
    );
}

#[test]
fn probe_class_with_symbol_computed_property() {
    // Well-known symbol as computed property name
    let output = emit_dts(
        r#"export class Iter {
    [Symbol.iterator](): Iterator<number> { return null!; }
}"#,
    );
    assert!(
        output.contains("[Symbol.iterator]"),
        "well-known symbol property: {output}"
    );
}

#[test]
fn probe_namespace_with_type_and_value() {
    // Namespace with both type and value exports
    let output = emit_dts(
        r#"export namespace NS {
    export interface Config { debug: boolean; }
    export function create(): Config;
    export const DEFAULT: Config;
}"#,
    );
    assert!(
        output.contains("interface Config"),
        "interface in ns: {output}"
    );
    assert!(
        output.contains("function create(): Config;"),
        "function in ns: {output}"
    );
    assert!(
        output.contains("const DEFAULT: Config;"),
        "const in ns: {output}"
    );
}

#[test]
fn probe_constructor_type_in_array_needs_parens() {
    // Constructor type in array: (new () => Foo)[]
    let output = emit_dts("export type CtorArr = (new () => object)[];");
    assert!(
        output.contains("(new () => object)[]"),
        "constructor type in array needs parens: {output}"
    );
}

#[test]
fn probe_conditional_type_in_array_needs_parens() {
    // Conditional type in array: (T extends U ? X : Y)[]
    let output = emit_dts("export type CondArr<T> = (T extends string ? 1 : 2)[];");
    assert!(
        output.contains("(T extends string ? 1 : 2)[]"),
        "conditional in array needs parens: {output}"
    );
}

#[test]
fn probe_intersection_type_in_array_needs_parens() {
    // Intersection type in array needs parens: (A & B)[]
    let output = emit_dts("export type InterArr = (string & { brand: true })[];");
    assert!(
        output.contains("(string &"),
        "intersection in array needs parens: {output}"
    );
}

#[test]
fn probe_type_operator_keyof_in_array_needs_parens() {
    // keyof in array: (keyof T)[]
    let output = emit_dts("export type Keys<T> = (keyof T)[];");
    // tsc emits: (keyof T)[]
    assert!(
        output.contains("(keyof T)[]"),
        "keyof type in array needs parens: {output}"
    );
}

#[test]
fn probe_infer_type_in_array_needs_parens() {
    // infer in conditional then used in array context
    let output = emit_dts("export type Flatten<T> = T extends (infer U)[] ? U : T;");
    assert!(
        output.contains("(infer U)[]"),
        "infer in array needs parens: {output}"
    );
}

#[test]
fn probe_function_type_in_intersection_needs_parens() {
    // Function type in intersection: ((x: number) => void) & { tag: true }
    let output = emit_dts(r#"export type TaggedFn = ((x: number) => void) & { tag: true };"#);
    assert!(
        output.contains("((x: number) => void) &"),
        "function type in intersection needs parens: {output}"
    );
}

#[test]
fn probe_conditional_check_type_parens() {
    // When the check type of a conditional is itself a union, it needs parens
    // Actually tsc doesn't parenthesize the check type of conditional differently
    // But when a function type is the check type, it does need parens
    let output =
        emit_dts("export type IsFn<T> = T extends (...args: any[]) => any ? true : false;");
    assert!(
        output.contains("(...args: any[]) => any ? true : false"),
        "function type as extends type in conditional: {output}"
    );
}

#[test]
fn probe_nested_type_literal_formatting() {
    // Nested type literal should have proper indentation
    let output = emit_dts(
        r#"export type Nested = {
    inner: {
        deep: string;
    };
};"#,
    );
    assert!(output.contains("inner:"), "nested inner prop: {output}");
    assert!(
        output.contains("deep: string;"),
        "nested deep prop: {output}"
    );
}

#[test]
fn probe_empty_interface() {
    // Empty interface
    let output = emit_dts("export interface Empty {}");
    assert!(
        output.contains("interface Empty {") && output.contains("}"),
        "empty interface: {output}"
    );
}

#[test]
fn probe_type_parameter_default_with_conditional() {
    // Type parameter with conditional default
    let output = emit_dts(
        "export type Wrap<T, R = T extends string ? string[] : T[]> = { value: T; result: R };",
    );
    assert!(
        output.contains("R = T extends string ? string[] : T[]"),
        "conditional type parameter default: {output}"
    );
}

#[test]
fn probe_function_returning_conditional() {
    // Function with conditional return type
    let output =
        emit_dts("export declare function check<T>(value: T): T extends string ? true : false;");
    assert!(
        output.contains("T extends string ? true : false"),
        "conditional return type: {output}"
    );
}

#[test]
fn probe_export_declare_enum_const() {
    // const enum
    let output = emit_dts(
        r#"export const enum Direction {
    Up = 0,
    Down = 1,
    Left = 2,
    Right = 3,
}"#,
    );
    assert!(
        output.contains("const enum Direction"),
        "const enum: {output}"
    );
}

#[test]
fn probe_interface_with_get_set_accessors() {
    // Get/set accessors in interface
    let output = emit_dts(
        r#"export interface HasAccessors {
    get value(): string;
    set value(v: string);
}"#,
    );
    assert!(
        output.contains("get value(): string;"),
        "get accessor in interface: {output}"
    );
    assert!(
        output.contains("set value(v: string);"),
        "set accessor in interface: {output}"
    );
}

#[test]
fn probe_method_overloads_skip_implementation() {
    // Method overloads should skip the implementation signature
    let output = emit_dts(
        r#"export class Overloaded {
    process(x: string): string;
    process(x: number): number;
    process(x: string | number): string | number { return x; }
}"#,
    );
    let process_count = output.matches("process(").count();
    assert_eq!(
        process_count, 2,
        "should have 2 overload signatures, got {process_count}: {output}"
    );
}

#[test]
fn probe_function_overloads_skip_implementation() {
    // Function overloads should skip the implementation signature
    let output = emit_dts(
        r#"export function parse(input: string): number;
export function parse(input: number): string;
export function parse(input: string | number): number | string {
    return typeof input === "string" ? 0 : "";
}"#,
    );
    let parse_count = output.matches("function parse(").count();
    assert_eq!(
        parse_count, 2,
        "should have 2 overload signatures, got {parse_count}: {output}"
    );
}

#[test]
fn probe_import_type_with_qualifier() {
    // import("module").Type.SubType
    let output = emit_dts(r#"export type X = import("./foo").Bar.Baz;"#);
    assert!(
        output.contains(r#"import("./foo").Bar.Baz"#),
        "import type with qualifier: {output}"
    );
}

#[test]
fn probe_import_type_with_type_args() {
    // import("module").Type<T>
    let output = emit_dts(r#"export type X = import("./foo").Container<string>;"#);
    assert!(
        output.contains(r#"import("./foo").Container<string>"#),
        "import type with type args: {output}"
    );
}

#[test]
fn probe_typeof_with_import() {
    // typeof import("module").default
    let output = emit_dts(r#"export type X = typeof import("./foo").default;"#);
    assert!(
        output.contains(r#"typeof import("./foo").default"#),
        "typeof import: {output}"
    );
}

#[test]
fn probe_class_with_multiple_heritage() {
    // Class with extends and implements
    let output = emit_dts(
        r#"interface Serializable { serialize(): string; }
interface Printable { print(): void; }
declare class Base { id: number; }
export class Child extends Base implements Serializable, Printable {
    serialize(): string { return ""; }
    print(): void {}
}"#,
    );
    assert!(
        output.contains("extends Base implements Serializable, Printable"),
        "extends + implements: {output}"
    );
}

#[test]
fn probe_template_literal_type_with_union() {
    // Template literal type containing union
    let output = emit_dts(r#"export type Event = `${"click" | "hover"}_${"start" | "end"}`;"#);
    // Should preserve the template literal structure
    assert!(
        output.contains("`${") && output.contains("}_${"),
        "template literal with unions: {output}"
    );
}

#[test]
fn probe_constructor_type_in_conditional_check_position() {
    // Constructor type in check position of conditional type needs parentheses.
    // Without parens: `new () => T extends U ? X : Y` would be parsed as
    // `new () => (T extends U ? X : Y)` rather than `(new () => T) extends U ? X : Y`.
    let output = emit_dts("export type X = (new () => T) extends U ? string : number;");
    assert!(
        output.contains("(new () => T) extends"),
        "constructor type should be parenthesized in check position: {output}"
    );
}

#[test]
fn probe_constructor_type_with_conditional_return_in_extends() {
    // Constructor type in conditional extends position with conditional return type
    // must be parenthesized to avoid ambiguous `extends` parsing.
    // Without parens: `T extends new () => U extends V ? A : B ? C : D`
    // would be mis-parsed as `T extends (new () => U) extends V ? A : B ? C : D`
    let output =
        emit_dts("export type X<T> = T extends (new () => (U extends V ? A : B)) ? C : D;");
    // The constructor type with conditional return should be parenthesized
    assert!(
        output.contains("(new () => U extends V ? A : B)")
            || output.contains("(new () => (U extends V ? A : B))"),
        "constructor type with conditional return should be parenthesized in extends: {output}"
    );
}

#[test]
fn take_diagnostics_drops_swapped_ts2883_when_canonical_exists() {
    use tsz_common::diagnostics::Diagnostic;

    let mut parser = ParserState::new("test.ts".to_string(), "".to_string());
    let _ = parser.parse_source_file();
    let mut emitter = DeclarationEmitter::new(&parser.arena);
    emitter.diagnostics.push(Diagnostic::from_code(
        2883,
        "src/index.ts",
        10,
        3,
        &["foo", "Other", "../node_modules/some-dep/dist/inner"],
    ));
    emitter.diagnostics.push(Diagnostic::from_code(
        2883,
        "src/index.ts",
        10,
        3,
        &["foo", "../node_modules/some-dep/dist/inner", "SomeType"],
    ));

    let diagnostics = emitter.take_diagnostics();
    assert_eq!(
        diagnostics.len(),
        1,
        "expected swapped TS2883 to be removed"
    );
    assert_eq!(
        diagnostics[0].message_text,
        "The inferred type of 'foo' cannot be named without a reference to 'SomeType' from '../node_modules/some-dep/dist/inner'. This is likely not portable. A type annotation is necessary."
    );
}

#[test]
fn take_diagnostics_keeps_ts2883_when_swapped_seen_before_canonical_duplicate() {
    use tsz_common::diagnostics::Diagnostic;

    let mut parser = ParserState::new("test.ts".to_string(), "".to_string());
    let _ = parser.parse_source_file();
    let mut emitter = DeclarationEmitter::new(&parser.arena);
    emitter.diagnostics.push(Diagnostic::from_code(
        2883,
        "src/index.ts",
        10,
        3,
        &["foo", "../node_modules/some-dep/dist/inner", "SomeType"],
    ));
    emitter.diagnostics.push(Diagnostic::from_code(
        2883,
        "src/index.ts",
        10,
        3,
        &["foo", "SomeType", "../node_modules/some-dep/dist/inner"],
    ));

    let diagnostics = emitter.take_diagnostics();
    assert_eq!(
        diagnostics.len(),
        1,
        "expected one surviving canonical TS2883 diagnostic"
    );
    assert_eq!(
        diagnostics[0].message_text,
        "The inferred type of 'foo' cannot be named without a reference to 'SomeType' from '../node_modules/some-dep/dist/inner'. This is likely not portable. A type annotation is necessary."
    );
}

// ── Private class namespace emission ────────────────────────────────

#[test]
fn test_private_class_not_emitted_in_module_namespace() {
    // Non-exported class inside a namespace should NOT appear in .d.ts
    // when no exported member references it.
    let result = emit_dts_with_usage_analysis(
        r#"
export namespace Ns {
    class privateClass { }
    export class publicClass { }

    // Not exported, not referenced by any export
    class privateClassWithPrivateModulePropertyTypes {
        myProperty: string;
    }

    export class publicClassWithPrivateModulePropertyTypes {
        myPublicProperty: string;
    }
}
"#,
    );
    assert!(
        !result.contains("privateClassWithPrivateModulePropertyTypes"),
        "private unreferenced class should not be emitted, got:\n{result}"
    );
    assert!(
        result.contains("publicClass"),
        "exported class should be emitted"
    );
    assert!(
        result.contains("publicClassWithPrivateModulePropertyTypes"),
        "exported class should be emitted"
    );
}

#[test]
fn test_private_class_emitted_when_referenced_by_export() {
    // Non-exported class inside a namespace SHOULD appear in .d.ts when
    // an exported member references it by name.
    let result = emit_dts_with_usage_analysis(
        r#"
export namespace Ns {
    class privateClass { }
    export class publicClass { }

    export interface publicInterfaceWithPrivatePropertyTypes {
        myProperty: privateClass;
    }
}
"#,
    );
    assert!(
        result.contains("class privateClass"),
        "referenced private class should be emitted, got:\n{result}"
    );
}

#[test]
fn test_private_class_not_emitted_at_module_top_level() {
    // Non-exported, non-referenced classes at the top level of a module
    // file must not leak into .d.ts.
    let result = emit_dts_with_usage_analysis(
        r#"
export class publicClass { }

class privateClassWithWithPublicPropertyTypes {
    myPublicProperty: publicClass;
}

export var publicVar: publicClass;
"#,
    );
    assert!(
        !result.contains("privateClassWithWithPublicPropertyTypes"),
        "unreferenced private top-level class should not be emitted, got:\n{result}"
    );
    assert!(
        result.contains("publicClass"),
        "exported class should be emitted"
    );
}

// ── TS2883 portability check: call expression return type ──────────

#[test]
fn check_call_expression_return_type_portability_skips_non_call() {
    // Verify that check_call_expression_return_type_portability does nothing
    // for non-call-expression initializers (e.g., simple identifiers).
    let mut parser = ParserState::new("test.ts".to_string(), "let x = 42;".to_string());
    let _ = parser.parse_source_file();
    let mut emitter = DeclarationEmitter::new(&parser.arena);

    // Should not panic or produce diagnostics for non-call nodes
    emitter.check_call_expression_return_type_portability(NodeIndex::NONE, "x", "test.ts", 4, 1);

    assert!(
        emitter.diagnostics.is_empty(),
        "no diagnostics expected for non-call initializers"
    );
}

#[test]
fn check_call_expression_return_type_portability_skip_when_disabled() {
    // Verify the check respects skip_portability_check flag
    let mut parser = ParserState::new("test.ts".to_string(), "".to_string());
    let _ = parser.parse_source_file();
    let mut emitter = DeclarationEmitter::new(&parser.arena);
    emitter.set_skip_portability_check(true);

    emitter.check_call_expression_return_type_portability(NodeIndex::NONE, "x", "test.ts", 0, 1);

    assert!(
        emitter.diagnostics.is_empty(),
        "no diagnostics when portability check is disabled"
    );
}

// ── TS2883 portability check: symlinked-monorepo nested package ─────

/// Build an empty `DeclarationEmitter` whose only purpose is exercising the
/// path-shape helpers (`strip_ts_extensions`, `calculate_relative_path`).
fn make_path_only_emitter<'a>(parser: &'a ParserState) -> DeclarationEmitter<'a> {
    DeclarationEmitter::new(&parser.arena)
}

#[test]
fn symlinked_nested_package_reference_fires_when_outer_package_is_not_consumer_ancestor() {
    // Structural shape: type's source path is `<X>/node_modules/<P>/<sub>` and
    // `<X>` is a sibling of (not an ancestor of) the consumer's directory. The
    // package was reached only through a nested / symlinked `node_modules` chain
    // outside the consumer's normal Node.js resolution scope, so writing `<P>`
    // as a bare specifier from the consumer would not resolve to the same file.
    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    let _ = parser.parse_source_file();
    let emitter = make_path_only_emitter(&parser);

    let result = emitter.symlinked_nested_package_reference(
        "Folder/monorepo/package-a/node_modules/styled-components/typings/styled-components.d.ts",
        "InterpolationValue",
        "Folder/monorepo/core/index.ts",
    );

    assert_eq!(
        result,
        Some((
            "../package-a/node_modules/styled-components/typings/styled-components".to_string(),
            "InterpolationValue".to_string(),
        )),
        "expected TS2883 reference for symlinked-monorepo nested package"
    );
}

#[test]
fn symlinked_nested_package_reference_independent_of_user_chosen_names() {
    // The fix must be structural: changing user-chosen package and type names
    // (the bound identifiers in this scenario) must not affect whether the
    // helper fires. Mirrors the failing test's shape with different names.
    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    let _ = parser.parse_source_file();
    let emitter = make_path_only_emitter(&parser);

    let result = emitter.symlinked_nested_package_reference(
        "repo/workspace/leaf-pkg/node_modules/dep-x/dist/types.d.ts",
        "Widget",
        "repo/workspace/consumer/main.ts",
    );

    assert_eq!(
        result,
        Some((
            "../leaf-pkg/node_modules/dep-x/dist/types".to_string(),
            "Widget".to_string(),
        )),
        "rule should be name-agnostic"
    );
}

#[test]
fn symlinked_nested_package_reference_skips_normal_node_modules_resolution() {
    // Normal resolution: the package's `<X>` is an ancestor of the consumer.
    // The helper must return None so the existing logic decides portability.
    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    let _ = parser.parse_source_file();
    let emitter = make_path_only_emitter(&parser);

    let result = emitter.symlinked_nested_package_reference(
        "project/node_modules/lib/index.d.ts",
        "Lib",
        "project/src/main.ts",
    );

    assert!(
        result.is_none(),
        "ancestor `<X>` should be left to the standard rules; got {result:?}"
    );
}

#[test]
fn symlinked_nested_package_reference_skips_paths_without_node_modules() {
    // Source paths without any `node_modules` segment (e.g. workspace siblings
    // resolved via symlinked package roots) are handled by other rules; this
    // helper must only consider node_modules-bearing source paths.
    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    let _ = parser.parse_source_file();
    let emitter = make_path_only_emitter(&parser);

    let result = emitter.symlinked_nested_package_reference(
        "workspace/packageA/index.d.ts",
        "Foo",
        "workspace/packageC/index.ts",
    );

    assert!(
        result.is_none(),
        "no node_modules in source path should yield None; got {result:?}"
    );
}

#[test]
fn symlinked_nested_package_reference_skips_multiple_node_modules() {
    // Source paths with two or more `node_modules` segments are already handled
    // by the existing nested-rules in `check_symbol_portability` (Cases 1 and 2).
    // The helper must defer to them by returning None.
    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    let _ = parser.parse_source_file();
    let emitter = make_path_only_emitter(&parser);

    let result = emitter.symlinked_nested_package_reference(
        "r/node_modules/foo/node_modules/nested/index.d.ts",
        "NestedProps",
        "r/entry.ts",
    );

    assert!(
        result.is_none(),
        "multi-node_modules paths must defer to existing rules; got {result:?}"
    );
}
