#[test]
fn test_parser_generic_arrow_with_constraint_and_default() {
    // Type parameter with both constraint and default
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const process = <T extends object = object>(x: T) => x;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_async_generic_arrow() {
    // Async generic arrow function
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const fetchData = async <T>(url: string) => { return url; };".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_generic_arrow_expression_body() {
    // Generic arrow with expression body
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const first = <T>(arr: T[]) => arr[0];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_arrow_function_with_return_type() {
    // Arrow function with return type annotation
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const add = (a: number, b: number): number => a + b;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_arrow_type_predicate() {
    // Arrow function with type predicate return type
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const isString = (x: unknown): x is string => typeof x === \"string\";".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_this_type_predicate() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "function isString(this: any): this is string { return true; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_asserts_this_type_predicate() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "function assertString(this: any): asserts this is string { }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_asserts_this_type_predicate_without_is() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "function assertThis(this: any): asserts this { }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_constructor_type() {
    // Constructor type: new () => T
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Ctor = new () => object;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_constructor_type_with_params() {
    // Constructor type with parameters: new (x: T) => U
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Factory<T> = new (value: T) => Wrapper<T>;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_generic_constructor_type() {
    // Generic constructor type: new <T>() => T
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type GenericCtor = new <T>() => T;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_keyof_type() {
    // Basic keyof type: keyof T
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Keys = keyof Person;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_keyof_typeof() {
    // keyof typeof: keyof typeof obj
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Keys = keyof typeof obj;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_keyof_in_union() {
    // keyof in union type
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type PropOrKey = string | keyof T;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_readonly_array() {
    // readonly array type
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let items: readonly string[];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_readonly_tuple() {
    // readonly tuple type
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let point: readonly [number, number];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_indexed_access_type() {
    // Basic indexed access: T[K]
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Value = Person[\"name\"];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_indexed_access_keyof() {
    // Indexed access with keyof: T[keyof T]
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Values = Person[keyof Person];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_indexed_access_chain() {
    // Chained indexed access: T[K1][K2]
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Deep = Obj[\"level1\"][\"level2\"];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_indexed_access_with_array() {
    // Mix of indexed access and array: T[K][]
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Names = Person[\"name\"][];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_indexed_access_number() {
    // Indexed access with number: T[number]
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Item = Items[number];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_conditional_type_simple() {
    // Basic conditional type: T extends U ? X : Y
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type IsString<T> = T extends string ? true : false;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_conditional_type_nested() {
    // Nested conditional types
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type TypeName<T> = T extends string ? \"string\" : T extends number ? \"number\" : \"other\";".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_conditional_type_with_infer() {
    // Conditional type with infer
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type ReturnType<T> = T extends (...args: any[]) => infer R ? R : never;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_conditional_type_distributive() {
    // Distributive conditional type
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type NonNullable<T> = T extends null | undefined ? never : T;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_infer_type() {
    // Infer in array element position
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Flatten<T> = T extends Array<infer U> ? U : T;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_mapped_type_simple() {
    // Basic mapped type: { [K in keyof T]: U }
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Partial<T> = { [K in keyof T]?: T[K] };".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_mapped_type_readonly() {
    // Mapped type with readonly modifier
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Readonly<T> = { readonly [K in keyof T]: T[K] };".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_mapped_type_required() {
    // Mapped type removing optional: -?
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Required<T> = { [K in keyof T]-?: T[K] };".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_mapped_type_as_clause() {
    // Mapped type with key remapping (as clause)
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Pick<T, K> = { [P in K as P]: T[P] };".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_type_literal() {
    // Object type literal
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Point = { x: number; y: number };".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_type_literal_method() {
    // Object type literal with method signature
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Calculator = { add(a: number, b: number): number; subtract(a: number, b: number): number };".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_template_literal_type_simple() {
    // Simple template literal type with no substitutions
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Greeting = `hello`;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_template_literal_type_with_substitution() {
    // Template literal type with type substitution
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Greeting<T extends string> = `hello ${T}`;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_template_literal_type_multiple_substitutions() {
    // Template literal type with multiple substitutions
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type FullName<F extends string, L extends string> = `${F} ${L}`;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_template_literal_type_with_union() {
    // Template literal type with union in substitution
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type EventName = `on${\"click\" | \"focus\" | \"blur\"}`;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_template_literal_type_uppercase() {
    // Template literal type with intrinsic type (Uppercase)
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Getter<K extends string> = `get${Uppercase<K>}`;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_jsx_self_closing() {
    // Self-closing JSX element
    let mut parser = ParserState::new(
        "test.tsx".to_string(),
        "const x = <Component />;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_jsx_with_children() {
    // JSX element with children
    let mut parser = ParserState::new(
        "test.tsx".to_string(),
        "const x = <div><span /></div>;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_jsx_with_attributes() {
    // JSX with attributes
    let mut parser = ParserState::new(
        "test.tsx".to_string(),
        "const x = <div className=\"foo\" id={bar} disabled />;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_jsx_with_expression() {
    // JSX with expression children
    let mut parser = ParserState::new(
        "test.tsx".to_string(),
        "const x = <div>{items.map(i => <span>{i}</span>)}</div>;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_jsx_fragment() {
    // JSX fragment
    let mut parser = ParserState::new(
        "test.tsx".to_string(),
        "const x = <><span /><span /></>;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_jsx_spread_attribute() {
    // JSX with spread attribute
    let mut parser = ParserState::new(
        "test.tsx".to_string(),
        "const x = <Component {...props} />;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_jsx_namespaced() {
    // JSX with namespaced tag name
    let mut parser = ParserState::new(
        "test.tsx".to_string(),
        "const x = <svg:rect width={100} />;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_jsx_member_expression() {
    // JSX with member expression tag
    let mut parser = ParserState::new(
        "test.tsx".to_string(),
        "const x = <Foo.Bar.Baz />;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_import_default() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        r#"import foo from "bar";"#.to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_import_named() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        r#"import { foo, bar } from "baz";"#.to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_import_namespace() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        r#"import * as foo from "bar";"#.to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_import_side_effect() {
    let mut parser = ParserState::new("test.ts".to_string(), r#"import "foo";"#.to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_export_function() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "export function foo() { return 1; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_export_const() {
    let mut parser = ParserState::new("test.ts".to_string(), "export const x = 42;".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_export_default() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "export default function foo() { }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_re_export() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        r#"export { foo } from "bar";"#.to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_default_re_export_specifiers() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        r#"export { default } from "bar"; export { default as Foo } from "bar";"#.to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_export_star() {
    let mut parser = ParserState::new("test.ts".to_string(), r#"export * from "foo";"#.to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_static_members() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Foo { static count: number = 0; static increment() { Foo.count++; } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    // May have diagnostics for static but should parse
}

#[test]
fn test_parser_private_protected() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Foo { private x: number; protected y: string; public z: boolean; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
}

#[test]
fn test_parser_readonly() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Foo { readonly name: string; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
}

#[test]
fn test_parser_constructor_parameter_properties() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Person { constructor(public name: string, private age: number) {} }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
}

#[test]
fn test_parser_optional_chaining() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let x = obj?.prop?.method?.()".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
}

#[test]
fn test_parser_optional_chain_call_with_type_arguments() {
    let (_parser, root) = parse_test_source("let x = obj?.<T>(value)");

    assert!(root.is_some());
}

#[test]
fn test_parser_relational_with_parenthesized_rhs() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "if (context.flags & NodeBuilderFlags.WriteTypeParametersInQualifiedName && index < (chain.length - 1)) { }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
}

#[test]
fn test_parser_every_type_arrow_conditional_comma() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "if (everyType(type, t => !!t.symbol?.parent && isArrayOrTupleSymbol(t.symbol.parent) && (!memberName ? (memberName = t.symbol.escapedName, true) : memberName === t.symbol.escapedName))) { }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_parser_every_type_arrow_conditional_comma_expression() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const ok = everyType(type, t => !!t.symbol?.parent && (!memberName ? (memberName = t.symbol.escapedName, true) : memberName === t.symbol.escapedName));".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_parser_checker_every_type_arrow_optional_chain() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let memberName: __String; if (everyType(type, t => !!t.symbol?.parent && isArrayOrTupleSymbol(t.symbol.parent) && (!memberName ? (memberName = t.symbol.escapedName, true) : memberName === t.symbol.escapedName))) { }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_parser_checker_every_type_arrow_optional_chain_line() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "if (everyType(type, t => !!t.symbol?.parent && isArrayOrTupleSymbol(t.symbol.parent) && (!memberName ? (memberName = t.symbol.escapedName, true) : memberName === t.symbol.escapedName))) { }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_parser_arrow_optional_chain_with_ternary_comma() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const f = (t: any) => !!t.symbol?.parent && (!memberName ? (memberName = t.symbol.escapedName, true) : memberName === t.symbol.escapedName);".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_parser_spread_in_call_arguments() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "foo(...args, 1, ...rest)".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
}

#[test]
fn test_parser_as_expression_followed_by_logical_or() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const x = (value as readonly number[] | undefined) || fallback".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
}

#[test]
fn test_parser_keyword_identifier_in_expression() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const set = new Set<number>(); set.add(1)".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
}

#[test]
fn test_parser_arrow_param_keyword_identifier() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const f = symbol => symbol".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
}

#[test]
fn test_parser_type_predicate_keyword_param() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "function isSymbol(symbol: unknown): symbol is Symbol { return true; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
}

#[test]
fn test_parser_namespace_identifier_assignment_statement() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let namespace = 1; namespace = 2;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
}

#[test]
fn test_parser_type_identifier_assignment_statement() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let type = { intrinsicName: \"\" }; type.intrinsicName = \"x\";".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
}

#[test]
fn test_parser_nullish_coalescing() {
    let (_parser, root) = parse_test_source("let x = a ?? b ?? c");

    assert!(root.is_some());
}

#[test]
fn test_parser_type_predicate() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "function isString(x: any): x is string { return typeof x === 'string'; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
}

#[test]
fn test_parser_mapped_type() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Readonly<T> = { readonly [K in keyof T]: T[K] }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
}

#[test]
fn test_parser_conditional_type() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type IsString<T> = T extends string ? true : false".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
}

#[test]
fn test_parser_infer_type_complex() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type ReturnType<T> = T extends (...args: any[]) => infer R ? R : never".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
}

#[test]
fn test_parser_rest_spread() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "function foo(...args: number[]) { let [first, ...rest] = args; return [...rest, first]; }"
            .to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
}

#[test]
fn test_parser_destructuring_default() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let { x = 1, y = 2 } = obj; let [a = 1, b = 2] = arr;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
}

#[test]
fn test_parser_computed_property() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let obj = { [key]: value, ['computed']: 42 }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
}

#[test]
fn test_parser_symbol_property() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let obj = { [Symbol.iterator]() { } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
}

#[test]
fn test_parser_bigint_literal() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let x: bigint = 123n; let y = 0xFFn;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
}

#[test]
fn test_parser_numeric_separator() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let x = 1_000_000; let y = 0xFF_FF_FF;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
}

#[test]
fn test_parser_private_identifier() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Foo { #privateField = 1; #privateMethod() {} }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
}

#[test]
fn test_parser_satisfies() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const obj = { x: 1, y: 2 } satisfies Record<string, number>".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
}

#[test]
fn test_parser_using_declaration() {
    // ECMAScript explicit resource management
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "using file = openFile(); await using conn = getConnection();".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
}

#[test]
fn test_parser_static_property() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Foo { static count = 0; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_static_method() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Foo { static create(): Foo { return new Foo(); } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_private_property() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Foo { private secret: string = 'hidden'; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_protected_method() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Base { protected init(): void {} }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_readonly_property() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Foo { readonly id: number = 1; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_public_constructor() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Foo { public constructor(x: number) {} }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_static_get_accessor() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Foo { static get instance(): Foo { return _instance; } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_private_set_accessor() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Foo { private set value(v: number) { this._value = v; } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_multiple_modifiers() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Foo { static readonly MAX_SIZE: number = 100; private static instance: Foo; }"
            .to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_override_method() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Child extends Parent { override doSomething(): void {} }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_async_method() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Foo { async fetchData(): Promise<void> {} }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_abstract_method_in_class() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "abstract class Shape { abstract getArea(): number; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_call_signature() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "interface Callable { (): string; (x: number): number; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_construct_signature() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "interface Constructable { new (): MyClass; new (x: number): MyClass; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}
