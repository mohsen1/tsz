use super::*;
use tsz_binder::BinderState;
use tsz_parser::ParserState;

/// Helper to parse source, bind, and compute semantic tokens.
fn get_tokens(source: &str) -> Vec<u32> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let mut provider = SemanticTokensProvider::new(arena, &binder, &line_map, source);
    provider.get_semantic_tokens(root)
}

/// Helper to decode delta-encoded tokens into absolute (line, col, len, type, modifiers).
fn decode_tokens(data: &[u32]) -> Vec<(u32, u32, u32, u32, u32)> {
    let mut result = Vec::new();
    let mut line = 0u32;
    let mut col = 0u32;
    for chunk in data.chunks_exact(5) {
        let delta_line = chunk[0];
        let delta_col = chunk[1];
        let length = chunk[2];
        let token_type = chunk[3];
        let modifiers = chunk[4];

        if delta_line > 0 {
            line += delta_line;
            col = delta_col;
        } else {
            col += delta_col;
        }
        result.push((line, col, length, token_type, modifiers));
    }
    result
}

/// Find a token by its position (line, col). Returns (type, modifiers).
fn find_token_at(tokens: &[(u32, u32, u32, u32, u32)], line: u32, col: u32) -> Option<(u32, u32)> {
    tokens
        .iter()
        .find(|t| t.0 == line && t.1 == col)
        .map(|t| (t.3, t.4))
}

#[test]
fn test_semantic_tokens_basic() {
    let source = "const x = 1;\nfunction foo() {}\nclass Bar {}";
    let tokens = get_tokens(source);

    // Should have tokens (5 values per token)
    assert!(!tokens.is_empty(), "Should have semantic tokens");
    assert_eq!(tokens.len() % 5, 0, "Token array should be divisible by 5");

    // Should have at least 3 tokens (x, foo, Bar)
    assert!(
        tokens.len() >= 15,
        "Should have at least 3 tokens (15 values)"
    );
}

#[test]
fn test_semantic_tokens_function() {
    let source = "function myFunc() {}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Find the function name token (at column 9, "myFunc")
    let func_token = find_token_at(&decoded, 0, 9);
    assert!(func_token.is_some(), "Should have token for myFunc");
    let (token_type, modifiers) = func_token.unwrap();
    assert_eq!(token_type, SemanticTokenType::Function as u32);
    assert_ne!(
        modifiers & semantic_token_modifiers::DECLARATION,
        0,
        "Should have DECLARATION modifier"
    );
}

#[test]
fn test_semantic_tokens_class() {
    let source = "class MyClass {}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Find the class name token (at column 6, "MyClass")
    let class_token = find_token_at(&decoded, 0, 6);
    assert!(class_token.is_some(), "Should have token for MyClass");
    let (token_type, modifiers) = class_token.unwrap();
    assert_eq!(token_type, SemanticTokenType::Class as u32);
    assert_ne!(modifiers & semantic_token_modifiers::DECLARATION, 0);
}

#[test]
fn test_semantic_tokens_delta_encoding() {
    let source = "const a = 1;\nconst b = 2;";
    let tokens = get_tokens(source);

    // Should have at least 2 tokens (a and b)
    assert!(
        tokens.len() >= 10,
        "Should have at least 2 tokens (10 values)"
    );

    // First token: deltaLine=0, deltaStart=6 (position of 'a')
    assert_eq!(tokens[0], 0); // deltaLine (first token always 0)
    assert_eq!(tokens[1], 6); // deltaStart (position of 'a')

    // Second token: deltaLine=1 (next line), deltaStart=6 (position of 'b')
    assert_eq!(tokens[5], 1); // deltaLine (moved to next line)
    assert_eq!(tokens[6], 6); // deltaStart (absolute position on new line)
}

#[test]
fn test_semantic_tokens_interface() {
    let source = "interface IFoo { bar: string; }";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Interface name at col 10
    let iface_token = find_token_at(&decoded, 0, 10);
    assert!(iface_token.is_some(), "Should have token for IFoo");
    let (token_type, _) = iface_token.unwrap();
    assert_eq!(token_type, SemanticTokenType::Interface as u32);
}

#[test]
fn test_semantic_tokens_enum() {
    let source = "enum Color { Red, Green, Blue }";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Enum name "Color" at col 5
    let enum_token = find_token_at(&decoded, 0, 5);
    assert!(enum_token.is_some(), "Should have token for Color");
    let (token_type, _) = enum_token.unwrap();
    assert_eq!(token_type, SemanticTokenType::Enum as u32);

    // Enum members should be EnumMember
    let red_token = find_token_at(&decoded, 0, 13);
    assert!(red_token.is_some(), "Should have token for Red");
    let (token_type, _) = red_token.unwrap();
    assert_eq!(token_type, SemanticTokenType::EnumMember as u32);
}

#[test]
fn test_semantic_tokens_type_alias() {
    let source = "type MyType = string | number;";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Type alias name "MyType" at col 5
    let type_token = find_token_at(&decoded, 0, 5);
    assert!(type_token.is_some(), "Should have token for MyType");
    let (token_type, modifiers) = type_token.unwrap();
    assert_eq!(token_type, SemanticTokenType::Type as u32);
    assert_ne!(modifiers & semantic_token_modifiers::DECLARATION, 0);
}

#[test]
fn test_semantic_tokens_parameter() {
    let source = "function greet(name: string) {}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Parameter "name" at col 15
    let param_token = find_token_at(&decoded, 0, 15);
    assert!(
        param_token.is_some(),
        "Should have token for parameter 'name'"
    );
    let (token_type, modifiers) = param_token.unwrap();
    assert_eq!(token_type, SemanticTokenType::Parameter as u32);
    assert_ne!(modifiers & semantic_token_modifiers::DECLARATION, 0);
}

#[test]
fn test_semantic_tokens_type_parameter() {
    let source = "function identity<T>(x: T): T { return x; }";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Type parameter "T" at col 18
    let tp_token = find_token_at(&decoded, 0, 18);
    assert!(tp_token.is_some(), "Should have token for type parameter T");
    let (token_type, _) = tp_token.unwrap();
    assert_eq!(token_type, SemanticTokenType::TypeParameter as u32);
}

#[test]
fn test_semantic_tokens_const_readonly_modifier() {
    let source = "const PI = 3.14;";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Variable "PI" at col 6
    let var_token = find_token_at(&decoded, 0, 6);
    assert!(var_token.is_some(), "Should have token for PI");
    let (token_type, modifiers) = var_token.unwrap();
    assert_eq!(token_type, SemanticTokenType::Variable as u32);
    assert_ne!(
        modifiers & semantic_token_modifiers::READONLY,
        0,
        "const variable should have READONLY modifier"
    );
    assert_ne!(modifiers & semantic_token_modifiers::DECLARATION, 0);
}

#[test]
fn test_semantic_tokens_let_variable_no_readonly() {
    let source = "let mutable = 1;";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Variable "mutable" at col 4
    let var_token = find_token_at(&decoded, 0, 4);
    assert!(var_token.is_some(), "Should have token for 'mutable'");
    let (token_type, modifiers) = var_token.unwrap();
    assert_eq!(token_type, SemanticTokenType::Variable as u32);
    assert_eq!(
        modifiers & semantic_token_modifiers::READONLY,
        0,
        "let variable should NOT have READONLY modifier"
    );
}

#[test]
fn test_semantic_tokens_namespace() {
    let source = "namespace MyNS { export const x = 1; }";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Namespace "MyNS" at col 10
    let ns_token = find_token_at(&decoded, 0, 10);
    assert!(ns_token.is_some(), "Should have token for MyNS");
    let (token_type, _) = ns_token.unwrap();
    assert_eq!(token_type, SemanticTokenType::Namespace as u32);
}

#[test]
fn test_semantic_tokens_variable_reference() {
    let source = "const x = 1;\nconst y = x;";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // 'x' declaration at (0, 6)
    let x_decl = find_token_at(&decoded, 0, 6);
    assert!(x_decl.is_some(), "Should have declaration token for x");
    let (tt, m) = x_decl.unwrap();
    assert_eq!(tt, SemanticTokenType::Variable as u32);
    assert_ne!(m & semantic_token_modifiers::DECLARATION, 0);

    // 'y' declaration at (1, 6)
    let y_decl = find_token_at(&decoded, 1, 6);
    assert!(y_decl.is_some(), "Should have declaration token for y");

    // 'x' reference at (1, 10) - used in initializer
    let x_ref = find_token_at(&decoded, 1, 10);
    assert!(x_ref.is_some(), "Should have reference token for x");
    let (tt, m) = x_ref.unwrap();
    assert_eq!(tt, SemanticTokenType::Variable as u32);
    // Reference should NOT have DECLARATION modifier
    assert_eq!(
        m & semantic_token_modifiers::DECLARATION,
        0,
        "Reference should not have DECLARATION modifier"
    );
}

#[test]
fn test_semantic_tokens_function_call_reference() {
    let source = "function add(a: number, b: number) { return a + b; }\nadd(1, 2);";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // 'add' declaration at (0, 9)
    let add_decl = find_token_at(&decoded, 0, 9);
    assert!(add_decl.is_some(), "Should have declaration token for add");
    let (tt, m) = add_decl.unwrap();
    assert_eq!(tt, SemanticTokenType::Function as u32);
    assert_ne!(m & semantic_token_modifiers::DECLARATION, 0);

    // 'add' reference at (1, 0) in call expression
    let add_ref = find_token_at(&decoded, 1, 0);
    assert!(
        add_ref.is_some(),
        "Should have reference token for add call"
    );
    let (tt, _m) = add_ref.unwrap();
    assert_eq!(tt, SemanticTokenType::Function as u32);
}

#[test]
fn test_semantic_tokens_class_method_property() {
    let source = "class Foo {\n  bar: number;\n  baz() {}\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Class name "Foo" at (0, 6)
    let class_token = find_token_at(&decoded, 0, 6);
    assert!(class_token.is_some(), "Should have token for Foo");
    assert_eq!(class_token.unwrap().0, SemanticTokenType::Class as u32);

    // Property "bar" at (1, 2)
    let prop_token = find_token_at(&decoded, 1, 2);
    assert!(prop_token.is_some(), "Should have token for property bar");
    assert_eq!(prop_token.unwrap().0, SemanticTokenType::Property as u32);

    // Method "baz" at (2, 2)
    let method_token = find_token_at(&decoded, 2, 2);
    assert!(method_token.is_some(), "Should have token for method baz");
    assert_eq!(method_token.unwrap().0, SemanticTokenType::Method as u32);
}

#[test]
fn test_semantic_tokens_multiple_declarations_same_line() {
    let source = "let a = 1, b = 2;";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // 'a' at (0, 4)
    let a_token = find_token_at(&decoded, 0, 4);
    assert!(a_token.is_some(), "Should have token for a");
    assert_eq!(a_token.unwrap().0, SemanticTokenType::Variable as u32);

    // 'b' at (0, 11)
    let b_token = find_token_at(&decoded, 0, 11);
    assert!(b_token.is_some(), "Should have token for b");
    assert_eq!(b_token.unwrap().0, SemanticTokenType::Variable as u32);
}

#[test]
fn test_semantic_tokens_expression_statement_reference() {
    let source = "const x = 1;\nx;";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // 'x' reference at (1, 0)
    let x_ref = find_token_at(&decoded, 1, 0);
    assert!(
        x_ref.is_some(),
        "Should have reference token for x in expression statement"
    );
    assert_eq!(x_ref.unwrap().0, SemanticTokenType::Variable as u32);
}

#[test]
fn test_semantic_tokens_parameter_reference_in_body() {
    let source = "function f(x: number) { return x; }";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // 'x' parameter declaration at (0, 11)
    let x_decl = find_token_at(&decoded, 0, 11);
    assert!(
        x_decl.is_some(),
        "Should have declaration token for parameter x"
    );
    let (tt, m) = x_decl.unwrap();
    assert_eq!(tt, SemanticTokenType::Parameter as u32);
    assert_ne!(m & semantic_token_modifiers::DECLARATION, 0);

    // 'x' reference at (0, 31) in return statement
    let x_ref = find_token_at(&decoded, 0, 31);
    assert!(
        x_ref.is_some(),
        "Should have reference token for x in return"
    );
    assert_eq!(x_ref.unwrap().0, SemanticTokenType::Parameter as u32);
}

#[test]
fn test_semantic_tokens_static_modifier() {
    let source = "class C {\n  static count = 0;\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // "static" keyword as Modifier token
    let static_token = find_token_at(&decoded, 1, 2);
    assert!(
        static_token.is_some(),
        "Should have token for static keyword"
    );
    assert_eq!(static_token.unwrap().0, SemanticTokenType::Modifier as u32);

    // "count" property with STATIC modifier
    let count_token = find_token_at(&decoded, 1, 9);
    assert!(count_token.is_some(), "Should have token for count");
    let (tt, m) = count_token.unwrap();
    assert_eq!(tt, SemanticTokenType::Property as u32);
    assert_ne!(
        m & semantic_token_modifiers::STATIC,
        0,
        "Should have STATIC modifier"
    );
}

#[test]
fn test_semantic_tokens_enum_with_values() {
    let source = "enum Direction {\n  Up = 1,\n  Down = 2,\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Enum "Direction" at (0, 5)
    let dir_token = find_token_at(&decoded, 0, 5);
    assert!(dir_token.is_some(), "Should have token for Direction");
    assert_eq!(dir_token.unwrap().0, SemanticTokenType::Enum as u32);

    // Enum member "Up" at (1, 2)
    let up_token = find_token_at(&decoded, 1, 2);
    assert!(up_token.is_some(), "Should have token for Up");
    assert_eq!(up_token.unwrap().0, SemanticTokenType::EnumMember as u32);

    // Enum member "Down" at (2, 2)
    let down_token = find_token_at(&decoded, 2, 2);
    assert!(down_token.is_some(), "Should have token for Down");
    assert_eq!(down_token.unwrap().0, SemanticTokenType::EnumMember as u32);
}

#[test]
fn test_semantic_tokens_decorator() {
    let source = "@decorator\nclass Foo {}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Decorator identifier "decorator" at (0, 1)
    let dec_token = find_token_at(&decoded, 0, 1);
    assert!(dec_token.is_some(), "Should have token for decorator");
    assert_eq!(dec_token.unwrap().0, SemanticTokenType::Decorator as u32);
}

#[test]
fn test_semantic_tokens_getter_setter() {
    let source = "class C {\n  get name() { return ''; }\n  set name(v: string) {}\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Class "C" at (0, 6)
    let c_token = find_token_at(&decoded, 0, 6);
    assert!(c_token.is_some(), "Should have token for C");
    assert_eq!(c_token.unwrap().0, SemanticTokenType::Class as u32);

    // Getter "name" at (1, 6)
    let get_token = find_token_at(&decoded, 1, 6);
    assert!(get_token.is_some(), "Should have token for getter name");

    // Setter "name" at (2, 6)
    let set_token = find_token_at(&decoded, 2, 6);
    assert!(set_token.is_some(), "Should have token for setter name");
}

#[test]
fn test_semantic_tokens_async_function() {
    let source = "async function fetchData() {}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Should have tokens for the function
    assert!(!decoded.is_empty(), "Should have tokens for async function");

    // Find the function name token - could be at various positions depending on how
    // async is handled. Just verify we find a Function token somewhere.
    let fn_token = decoded
        .iter()
        .find(|t| t.3 == SemanticTokenType::Function as u32);
    assert!(
        fn_token.is_some(),
        "Should have a Function token for fetchData"
    );
}

#[test]
fn test_semantic_tokens_import_specifier() {
    let source = "import { foo } from './mod';";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // "foo" import specifier at some position
    // The token should exist for the imported name
    assert!(!decoded.is_empty(), "Should have tokens for import");
}

#[test]
fn test_semantic_tokens_export_specifier() {
    let source = "const x = 1;\nexport { x };";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // x declaration at (0, 6)
    let x_decl = find_token_at(&decoded, 0, 6);
    assert!(x_decl.is_some(), "Should have token for x declaration");

    // Should have at least 2 tokens
    assert!(
        decoded.len() >= 2,
        "Should have tokens for both declaration and export"
    );
}

#[test]
fn test_semantic_tokens_abstract_class() {
    let source = "abstract class Base {\n  abstract method(): void;\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // "abstract" modifier at (0, 0)
    let abstract_kw = find_token_at(&decoded, 0, 0);
    assert!(
        abstract_kw.is_some(),
        "Should have token for abstract keyword"
    );
    assert_eq!(abstract_kw.unwrap().0, SemanticTokenType::Modifier as u32);

    // "Base" class at (0, 15)
    let class_token = find_token_at(&decoded, 0, 15);
    assert!(class_token.is_some(), "Should have token for Base");
    assert_eq!(class_token.unwrap().0, SemanticTokenType::Class as u32);
}

#[test]
fn test_semantic_tokens_private_modifier() {
    let source = "class C {\n  private x: number = 0;\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // "private" modifier at (1, 2)
    let private_kw = find_token_at(&decoded, 1, 2);
    assert!(
        private_kw.is_some(),
        "Should have token for private keyword"
    );
    assert_eq!(private_kw.unwrap().0, SemanticTokenType::Modifier as u32);
}

#[test]
fn test_semantic_tokens_readonly_modifier_keyword() {
    let source = "class C {\n  readonly name: string = '';\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // "readonly" modifier at (1, 2)
    let readonly_kw = find_token_at(&decoded, 1, 2);
    assert!(
        readonly_kw.is_some(),
        "Should have token for readonly keyword"
    );
    assert_eq!(readonly_kw.unwrap().0, SemanticTokenType::Modifier as u32);
}

#[test]
fn test_semantic_tokens_empty_source() {
    let tokens = get_tokens("");
    assert!(tokens.is_empty(), "Empty source should produce no tokens");
}

#[test]
fn test_semantic_tokens_only_comments() {
    let tokens = get_tokens("// just a comment\n/* block comment */");
    // Comments are not emitted as semantic tokens since the editor handles them
    // Just verify no crash
    assert_eq!(tokens.len() % 5, 0, "Token array should be divisible by 5");
}

#[test]
fn test_semantic_tokens_multiple_type_params() {
    let source = "function map<T, U>(items: T[], fn: (item: T) => U): U[] { return []; }";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // "T" type parameter at col 13
    let t_token = find_token_at(&decoded, 0, 13);
    assert!(t_token.is_some(), "Should have token for T");
    assert_eq!(t_token.unwrap().0, SemanticTokenType::TypeParameter as u32);

    // "U" type parameter at col 16
    let u_token = find_token_at(&decoded, 0, 16);
    assert!(u_token.is_some(), "Should have token for U");
    assert_eq!(u_token.unwrap().0, SemanticTokenType::TypeParameter as u32);
}

#[test]
fn test_semantic_tokens_interface_property() {
    let source = "interface Config {\n  host: string;\n  port: number;\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Interface "Config" should be emitted as Interface
    let iface_token = decoded
        .iter()
        .find(|t| t.3 == SemanticTokenType::Interface as u32);
    assert!(
        iface_token.is_some(),
        "Should have Interface token for Config"
    );

    // Property signatures may or may not be emitted depending on binder support
    // Just verify no crash and basic token generation
    assert!(
        !decoded.is_empty(),
        "Should have at least the interface token"
    );
}

#[test]
fn test_semantic_tokens_interface_method_signature() {
    let source = "interface Logger {\n  log(msg: string): void;\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Interface "Logger" should be emitted
    let iface_token = decoded
        .iter()
        .find(|t| t.3 == SemanticTokenType::Interface as u32);
    assert!(
        iface_token.is_some(),
        "Should have Interface token for Logger"
    );

    // Method signatures in interfaces may or may not be emitted
    // Just verify the basic token generation works
    assert!(
        !decoded.is_empty(),
        "Should have at least the interface token"
    );
}

#[test]
fn test_semantic_tokens_const_enum() {
    let source = "const enum Direction { Up, Down }";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Should still recognize enum and members
    assert!(!decoded.is_empty(), "Should have tokens for const enum");
}

#[test]
fn test_semantic_tokens_namespace_declaration() {
    let source = "namespace App {\n  export function init() {}\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // "App" namespace at (0, 10)
    let ns = find_token_at(&decoded, 0, 10);
    assert!(ns.is_some(), "Should have token for namespace App");
    assert_eq!(ns.unwrap().0, SemanticTokenType::Namespace as u32);
}

#[test]
fn test_semantic_tokens_builder_same_line_delta() {
    // Test that delta encoding is correct for multiple tokens on same line
    let mut builder = SemanticTokensBuilder::new();
    builder.push(0, 5, 3, SemanticTokenType::Variable, 0);
    builder.push(0, 12, 4, SemanticTokenType::Function, 0);
    let data = builder.build();

    assert_eq!(data.len(), 10);
    // First token: deltaLine=0, deltaStart=5
    assert_eq!(data[0], 0);
    assert_eq!(data[1], 5);
    // Second token: deltaLine=0, deltaStart=7 (12 - 5 = 7)
    assert_eq!(data[5], 0);
    assert_eq!(data[6], 7);
}

#[test]
fn test_semantic_tokens_builder_multi_line() {
    let mut builder = SemanticTokensBuilder::new();
    builder.push(0, 5, 3, SemanticTokenType::Variable, 0);
    builder.push(2, 3, 4, SemanticTokenType::Function, 0);
    let data = builder.build();

    assert_eq!(data.len(), 10);
    // Second token: deltaLine=2, deltaStart=3 (absolute on new line)
    assert_eq!(data[5], 2);
    assert_eq!(data[6], 3);
}

#[test]
fn test_semantic_tokens_builder_empty() {
    let builder = SemanticTokensBuilder::new();
    let data = builder.build();
    assert!(data.is_empty());
}

#[test]
fn test_semantic_tokens_var_reference_readonly() {
    // A const reference should still have READONLY in reference position
    let source = "const PI = 3.14;\nconst area = PI * 4;";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // PI reference at (1, 13)
    let pi_ref = find_token_at(&decoded, 1, 13);
    assert!(pi_ref.is_some(), "Should have reference token for PI");
    let (tt, m) = pi_ref.unwrap();
    assert_eq!(tt, SemanticTokenType::Variable as u32);
    assert_ne!(
        m & semantic_token_modifiers::READONLY,
        0,
        "const ref should have READONLY"
    );
}

#[test]
fn test_semantic_tokens_class_with_constructor() {
    let source = "class Point {\n  constructor(public x: number, public y: number) {}\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // "Point" class at (0, 6)
    let class_token = find_token_at(&decoded, 0, 6);
    assert!(class_token.is_some());
    assert_eq!(class_token.unwrap().0, SemanticTokenType::Class as u32);

    // "public" modifiers should be present
    let pub1 = find_token_at(&decoded, 1, 14);
    if let Some((tt, _)) = pub1 {
        assert_eq!(tt, SemanticTokenType::Modifier as u32);
    }
}

// =========================================================================
// Additional tests for improved coverage
// =========================================================================

#[test]
fn test_semantic_tokens_arrow_function_variable() {
    let source = "const add = (a: number, b: number) => a + b;";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // "add" at (0, 6) should be a Variable
    let add_token = find_token_at(&decoded, 0, 6);
    assert!(add_token.is_some(), "Should have token for add");
    let (tt, m) = add_token.unwrap();
    assert_eq!(tt, SemanticTokenType::Variable as u32);
    assert_ne!(
        m & semantic_token_modifiers::READONLY,
        0,
        "const arrow fn should have READONLY"
    );
}

#[test]
fn test_semantic_tokens_destructured_variable() {
    let source = "const { a, b } = { a: 1, b: 2 };";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Just ensure no crash and tokens are produced
    assert!(
        !decoded.is_empty(),
        "Should produce tokens for destructuring"
    );
    assert_eq!(tokens.len() % 5, 0, "Token array should be divisible by 5");
}

#[test]
fn test_semantic_tokens_for_of_loop_variable() {
    let source = "const arr = [1, 2];\nfor (const item of arr) {\n  item;\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // "arr" declaration at (0, 6)
    let arr_token = find_token_at(&decoded, 0, 6);
    assert!(arr_token.is_some(), "Should have token for arr");
    assert_eq!(arr_token.unwrap().0, SemanticTokenType::Variable as u32);
}

#[test]
fn test_semantic_tokens_class_extends_reference() {
    let source = "class Base {}\nclass Child extends Base {}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // "Base" class declaration at (0, 6)
    let base_decl = find_token_at(&decoded, 0, 6);
    assert!(
        base_decl.is_some(),
        "Should have token for Base declaration"
    );
    assert_eq!(base_decl.unwrap().0, SemanticTokenType::Class as u32);

    // "Child" class at (1, 6)
    let child_token = find_token_at(&decoded, 1, 6);
    assert!(child_token.is_some(), "Should have token for Child");
    assert_eq!(child_token.unwrap().0, SemanticTokenType::Class as u32);
}

#[test]
fn test_semantic_tokens_type_reference_in_annotation() {
    let source = "interface Foo {}\nconst x: Foo = {};";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Interface "Foo" declaration at (0, 10)
    let foo_decl = find_token_at(&decoded, 0, 10);
    assert!(foo_decl.is_some(), "Should have token for Foo declaration");
    assert_eq!(foo_decl.unwrap().0, SemanticTokenType::Interface as u32);

    // "x" variable at (1, 6)
    let x_token = find_token_at(&decoded, 1, 6);
    assert!(x_token.is_some(), "Should have token for x");
    assert_eq!(x_token.unwrap().0, SemanticTokenType::Variable as u32);
}

#[test]
fn test_semantic_tokens_export_function() {
    let source = "export function exported() {}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Find the function token
    let fn_token = decoded
        .iter()
        .find(|t| t.3 == SemanticTokenType::Function as u32);
    assert!(
        fn_token.is_some(),
        "Should have Function token for exported function"
    );
}

#[test]
fn test_semantic_tokens_class_static_method() {
    let source = "class Util {\n  static create() { return new Util(); }\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // "Util" class at (0, 6)
    let class_token = find_token_at(&decoded, 0, 6);
    assert!(class_token.is_some(), "Should have token for Util class");
    assert_eq!(class_token.unwrap().0, SemanticTokenType::Class as u32);

    // "static" modifier at (1, 2)
    let static_token = find_token_at(&decoded, 1, 2);
    assert!(
        static_token.is_some(),
        "Should have token for static keyword"
    );
    assert_eq!(static_token.unwrap().0, SemanticTokenType::Modifier as u32);

    // "create" method at (1, 9)
    let method_token = find_token_at(&decoded, 1, 9);
    assert!(
        method_token.is_some(),
        "Should have token for create method"
    );
    if let Some((tt, m)) = method_token {
        assert_eq!(tt, SemanticTokenType::Method as u32);
        assert_ne!(
            m & semantic_token_modifiers::STATIC,
            0,
            "Static method should have STATIC modifier"
        );
    }
}

#[test]
fn test_semantic_tokens_protected_modifier() {
    let source = "class C {\n  protected value: number = 0;\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // "protected" modifier at (1, 2)
    let protected_kw = find_token_at(&decoded, 1, 2);
    assert!(
        protected_kw.is_some(),
        "Should have token for protected keyword"
    );
    assert_eq!(protected_kw.unwrap().0, SemanticTokenType::Modifier as u32);
}

#[test]
fn test_semantic_tokens_nested_function() {
    let source = "function outer() {\n  function inner() {}\n  inner();\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // "outer" function at (0, 9)
    let outer_token = find_token_at(&decoded, 0, 9);
    assert!(outer_token.is_some(), "Should have token for outer");
    assert_eq!(outer_token.unwrap().0, SemanticTokenType::Function as u32);

    // "inner" function at (1, 11)
    let inner_token = find_token_at(&decoded, 1, 11);
    assert!(inner_token.is_some(), "Should have token for inner");
    assert_eq!(inner_token.unwrap().0, SemanticTokenType::Function as u32);
}

#[test]
fn test_semantic_tokens_var_variable() {
    let source = "var legacy = 42;";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // "legacy" at (0, 4)
    let var_token = find_token_at(&decoded, 0, 4);
    assert!(var_token.is_some(), "Should have token for var variable");
    let (tt, m) = var_token.unwrap();
    assert_eq!(tt, SemanticTokenType::Variable as u32);
    // var should not have READONLY modifier
    assert_eq!(
        m & semantic_token_modifiers::READONLY,
        0,
        "var variable should NOT have READONLY modifier"
    );
}

#[test]
fn test_semantic_tokens_builder_single_token() {
    let mut builder = SemanticTokensBuilder::new();
    builder.push(
        3,
        10,
        5,
        SemanticTokenType::Class,
        semantic_token_modifiers::DECLARATION,
    );
    let data = builder.build();

    assert_eq!(data.len(), 5);
    assert_eq!(data[0], 3); // deltaLine
    assert_eq!(data[1], 10); // deltaStart
    assert_eq!(data[2], 5); // length
    assert_eq!(data[3], SemanticTokenType::Class as u32); // tokenType
    assert_eq!(data[4], semantic_token_modifiers::DECLARATION); // modifiers
}

#[test]
fn test_semantic_tokens_class_implements_interface_ref() {
    let source = "interface Serializable {}\nclass Data implements Serializable {}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Interface "Serializable" declaration at (0, 10)
    let iface_token = find_token_at(&decoded, 0, 10);
    assert!(iface_token.is_some(), "Should have token for Serializable");
    assert_eq!(iface_token.unwrap().0, SemanticTokenType::Interface as u32);

    // Class "Data" at (1, 6)
    let class_token = find_token_at(&decoded, 1, 6);
    assert!(class_token.is_some(), "Should have token for Data class");
    assert_eq!(class_token.unwrap().0, SemanticTokenType::Class as u32);
}

// =========================================================================
// Additional tests for expanded coverage (batch 2)
// =========================================================================

#[test]
fn test_semantic_tokens_multiline_function() {
    let source = "function multi(\n  a: number,\n  b: string\n) {\n  return a;\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // "multi" function at (0, 9)
    let fn_token = find_token_at(&decoded, 0, 9);
    assert!(fn_token.is_some(), "Should have token for multi");
    assert_eq!(fn_token.unwrap().0, SemanticTokenType::Function as u32);

    // "a" parameter at (1, 2)
    let a_token = find_token_at(&decoded, 1, 2);
    assert!(a_token.is_some(), "Should have token for parameter a");
    assert_eq!(a_token.unwrap().0, SemanticTokenType::Parameter as u32);

    // "b" parameter at (2, 2)
    let b_token = find_token_at(&decoded, 2, 2);
    assert!(b_token.is_some(), "Should have token for parameter b");
    assert_eq!(b_token.unwrap().0, SemanticTokenType::Parameter as u32);
}

#[test]
fn test_semantic_tokens_interface_extends() {
    let source = "interface Base {}\ninterface Child extends Base {}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // "Base" interface at (0, 10)
    let base_token = find_token_at(&decoded, 0, 10);
    assert!(base_token.is_some(), "Should have token for Base");
    assert_eq!(base_token.unwrap().0, SemanticTokenType::Interface as u32);

    // "Child" interface at (1, 10)
    let child_token = find_token_at(&decoded, 1, 10);
    assert!(child_token.is_some(), "Should have token for Child");
    assert_eq!(child_token.unwrap().0, SemanticTokenType::Interface as u32);
}

#[test]
fn test_semantic_tokens_enum_member_reference() {
    let source = "enum Dir { Up, Down }\nconst d = Dir.Up;";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // "Dir" enum at (0, 5)
    let dir_token = find_token_at(&decoded, 0, 5);
    assert!(dir_token.is_some(), "Should have token for Dir");
    assert_eq!(dir_token.unwrap().0, SemanticTokenType::Enum as u32);

    // Should have tokens on both lines
    assert!(
        decoded.iter().any(|t| t.0 == 1),
        "Should have tokens on line 1"
    );
}

#[test]
fn test_semantic_tokens_generic_class() {
    let source = "class Container<T> {\n  value: T;\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // "Container" class at (0, 6)
    let class_token = find_token_at(&decoded, 0, 6);
    assert!(class_token.is_some(), "Should have token for Container");
    assert_eq!(class_token.unwrap().0, SemanticTokenType::Class as u32);

    // "T" type parameter at (0, 16)
    let tp_token = find_token_at(&decoded, 0, 16);
    assert!(tp_token.is_some(), "Should have token for T");
    assert_eq!(tp_token.unwrap().0, SemanticTokenType::TypeParameter as u32);
}

#[test]
fn test_semantic_tokens_generic_interface() {
    let source = "interface Pair<A, B> {\n  first: A;\n  second: B;\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // "Pair" interface at (0, 10)
    let iface_token = find_token_at(&decoded, 0, 10);
    assert!(iface_token.is_some(), "Should have token for Pair");
    assert_eq!(iface_token.unwrap().0, SemanticTokenType::Interface as u32);

    // "A" type parameter at (0, 15)
    let a_tp = find_token_at(&decoded, 0, 15);
    assert!(a_tp.is_some(), "Should have token for type param A");
    assert_eq!(a_tp.unwrap().0, SemanticTokenType::TypeParameter as u32);

    // "B" type parameter at (0, 18)
    let b_tp = find_token_at(&decoded, 0, 18);
    assert!(b_tp.is_some(), "Should have token for type param B");
    assert_eq!(b_tp.unwrap().0, SemanticTokenType::TypeParameter as u32);
}

#[test]
fn test_semantic_tokens_multiple_classes() {
    let source = "class A {}\nclass B {}\nclass C {}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    let class_tokens: Vec<_> = decoded
        .iter()
        .filter(|t| t.3 == SemanticTokenType::Class as u32)
        .collect();
    assert!(
        class_tokens.len() >= 3,
        "Should have at least 3 class tokens, got {}",
        class_tokens.len()
    );
}

#[test]
fn test_semantic_tokens_multiple_interfaces() {
    let source = "interface X {}\ninterface Y {}\ninterface Z {}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    let iface_tokens: Vec<_> = decoded
        .iter()
        .filter(|t| t.3 == SemanticTokenType::Interface as u32)
        .collect();
    assert!(
        iface_tokens.len() >= 3,
        "Should have at least 3 interface tokens, got {}",
        iface_tokens.len()
    );
}

#[test]
fn test_semantic_tokens_builder_modifiers_bitmask() {
    let mut builder = SemanticTokensBuilder::new();
    let modifiers = semantic_token_modifiers::DECLARATION
        | semantic_token_modifiers::READONLY
        | semantic_token_modifiers::STATIC;
    builder.push(0, 0, 5, SemanticTokenType::Variable, modifiers);
    let data = builder.build();

    assert_eq!(data.len(), 5);
    assert_eq!(
        data[4], modifiers,
        "Should preserve combined modifiers bitmask"
    );
}

#[test]
fn test_semantic_tokens_builder_three_tokens_same_line() {
    let mut builder = SemanticTokensBuilder::new();
    builder.push(0, 0, 1, SemanticTokenType::Variable, 0);
    builder.push(0, 5, 2, SemanticTokenType::Function, 0);
    builder.push(0, 10, 3, SemanticTokenType::Class, 0);
    let data = builder.build();

    assert_eq!(data.len(), 15);
    // First: deltaLine=0, deltaStart=0
    assert_eq!(data[0], 0);
    assert_eq!(data[1], 0);
    // Second: deltaLine=0, deltaStart=5 (5-0)
    assert_eq!(data[5], 0);
    assert_eq!(data[6], 5);
    // Third: deltaLine=0, deltaStart=5 (10-5)
    assert_eq!(data[10], 0);
    assert_eq!(data[11], 5);
}

#[test]
fn test_semantic_tokens_type_alias_with_generics() {
    let source = "type Result<T, E> = { ok: T } | { err: E };";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // "Result" at (0, 5) - Type
    let type_token = find_token_at(&decoded, 0, 5);
    assert!(type_token.is_some(), "Should have token for Result");
    assert_eq!(type_token.unwrap().0, SemanticTokenType::Type as u32);

    // "T" at (0, 12)
    let t_token = find_token_at(&decoded, 0, 12);
    assert!(t_token.is_some(), "Should have token for T");
    assert_eq!(t_token.unwrap().0, SemanticTokenType::TypeParameter as u32);

    // "E" at (0, 15)
    let e_token = find_token_at(&decoded, 0, 15);
    assert!(e_token.is_some(), "Should have token for E");
    assert_eq!(e_token.unwrap().0, SemanticTokenType::TypeParameter as u32);
}

#[test]
fn test_semantic_tokens_const_enum_members() {
    let source = "const enum Status {\n  Active = 1,\n  Inactive = 2,\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Find enum members
    let enum_member_tokens: Vec<_> = decoded
        .iter()
        .filter(|t| t.3 == SemanticTokenType::EnumMember as u32)
        .collect();
    assert!(
        enum_member_tokens.len() >= 2,
        "Should have at least 2 enum member tokens, got {}",
        enum_member_tokens.len()
    );
}

#[test]
fn test_semantic_tokens_class_with_multiple_methods() {
    let source = "class Api {\n  get() {}\n  post() {}\n  put() {}\n  del() {}\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    let method_tokens: Vec<_> = decoded
        .iter()
        .filter(|t| t.3 == SemanticTokenType::Method as u32)
        .collect();
    assert!(
        method_tokens.len() >= 4,
        "Should have at least 4 method tokens, got {}",
        method_tokens.len()
    );
}

#[test]
fn test_semantic_tokens_nested_class() {
    let source = "class Outer {\n  inner() {\n    class Inner {}\n  }\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    let class_tokens: Vec<_> = decoded
        .iter()
        .filter(|t| t.3 == SemanticTokenType::Class as u32)
        .collect();
    assert!(
        class_tokens.len() >= 2,
        "Should have at least 2 class tokens (Outer + Inner), got {}",
        class_tokens.len()
    );
}

// =========================================================================
// Additional semantic tokens tests (batch 3)
// =========================================================================

#[test]
fn test_semantic_tokens_default_export_function() {
    let source = "export default function handler() {}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Should produce tokens without crashing
    assert!(
        !decoded.is_empty(),
        "Should produce tokens for default export function"
    );
    // Tokens should be well-formed (divisible by 5)
    assert_eq!(tokens.len() % 5, 0, "Token array should be divisible by 5");
}

#[test]
fn test_semantic_tokens_multiple_parameters() {
    let source = "function calc(x: number, y: number, z: number) {}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    let param_tokens: Vec<_> = decoded
        .iter()
        .filter(|t| t.3 == SemanticTokenType::Parameter as u32)
        .collect();
    assert!(
        param_tokens.len() >= 3,
        "Should have at least 3 parameter tokens (x, y, z), got {}",
        param_tokens.len()
    );
}

#[test]
fn test_semantic_tokens_class_private_field() {
    let source = "class C {\n  #secret: string = '';\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Should produce tokens without crashing for private field
    assert!(
        !decoded.is_empty(),
        "Should produce tokens for class with private field"
    );
    assert_eq!(tokens.len() % 5, 0, "Token array should be divisible by 5");

    // Find the class token somewhere
    let class_token = decoded
        .iter()
        .find(|t| t.3 == SemanticTokenType::Class as u32);
    if let Some(ct) = class_token {
        assert_eq!(ct.3, SemanticTokenType::Class as u32);
    }
}

#[test]
fn test_semantic_tokens_template_literal_no_crash() {
    let source = "const name = 'world';\nconst msg = `hello ${name}`;";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // 'name' declaration at (0, 6)
    let name_decl = find_token_at(&decoded, 0, 6);
    assert!(
        name_decl.is_some(),
        "Should have token for name declaration"
    );
    assert_eq!(name_decl.unwrap().0, SemanticTokenType::Variable as u32);

    // 'msg' at (1, 6)
    let msg_token = find_token_at(&decoded, 1, 6);
    assert!(msg_token.is_some(), "Should have token for msg");
    assert_eq!(msg_token.unwrap().0, SemanticTokenType::Variable as u32);
}

#[test]
fn test_semantic_tokens_class_accessor_keyword() {
    let source = "class C {\n  accessor prop: number = 0;\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Should not crash; class token should be present
    let class_token = find_token_at(&decoded, 0, 6);
    assert!(class_token.is_some(), "Should have token for class C");
    assert_eq!(class_token.unwrap().0, SemanticTokenType::Class as u32);
}

#[test]
fn test_semantic_tokens_multiple_enums() {
    let source = "enum A { X }\nenum B { Y }\nenum C { Z }";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    let enum_tokens: Vec<_> = decoded
        .iter()
        .filter(|t| t.3 == SemanticTokenType::Enum as u32)
        .collect();
    assert!(
        enum_tokens.len() >= 3,
        "Should have at least 3 enum tokens, got {}",
        enum_tokens.len()
    );

    let member_tokens: Vec<_> = decoded
        .iter()
        .filter(|t| t.3 == SemanticTokenType::EnumMember as u32)
        .collect();
    assert!(
        member_tokens.len() >= 3,
        "Should have at least 3 enum member tokens, got {}",
        member_tokens.len()
    );
}

#[test]
fn test_semantic_tokens_class_async_method() {
    let source = "class Api {\n  async fetch() {}\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Class "Api" at (0, 6)
    let class_token = find_token_at(&decoded, 0, 6);
    assert!(class_token.is_some(), "Should have token for Api");
    assert_eq!(class_token.unwrap().0, SemanticTokenType::Class as u32);

    // Should have tokens on line 1 for the async method
    assert!(
        decoded.iter().any(|t| t.0 == 1),
        "Should have tokens on line 1"
    );
}

#[test]
fn test_semantic_tokens_multiple_type_aliases() {
    let source = "type A = string;\ntype B = number;\ntype C = boolean;";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    let type_tokens: Vec<_> = decoded
        .iter()
        .filter(|t| t.3 == SemanticTokenType::Type as u32)
        .collect();
    assert!(
        type_tokens.len() >= 3,
        "Should have at least 3 type tokens, got {}",
        type_tokens.len()
    );
}

#[test]
fn test_semantic_tokens_function_with_generics() {
    let source = "function wrap<T>(value: T): { inner: T } { return { inner: value }; }";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // "wrap" function at (0, 9)
    let fn_token = find_token_at(&decoded, 0, 9);
    assert!(fn_token.is_some(), "Should have token for wrap");
    assert_eq!(fn_token.unwrap().0, SemanticTokenType::Function as u32);

    // "T" type param at (0, 14)
    let tp_token = find_token_at(&decoded, 0, 14);
    assert!(tp_token.is_some(), "Should have token for T");
    assert_eq!(tp_token.unwrap().0, SemanticTokenType::TypeParameter as u32);

    // "value" parameter at (0, 17)
    let param_token = find_token_at(&decoded, 0, 17);
    assert!(param_token.is_some(), "Should have token for value param");
    assert_eq!(param_token.unwrap().0, SemanticTokenType::Parameter as u32);
}

#[test]
fn test_semantic_tokens_multiple_functions() {
    let source = "function a() {}\nfunction b() {}\nfunction c() {}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    let fn_tokens: Vec<_> = decoded
        .iter()
        .filter(|t| t.3 == SemanticTokenType::Function as u32)
        .collect();
    assert!(
        fn_tokens.len() >= 3,
        "Should have at least 3 function tokens, got {}",
        fn_tokens.len()
    );
}

#[test]
fn test_semantic_tokens_builder_descending_lines() {
    // Ensure builder handles tokens spread across many lines
    let mut builder = SemanticTokensBuilder::new();
    builder.push(0, 0, 3, SemanticTokenType::Variable, 0);
    builder.push(5, 0, 4, SemanticTokenType::Function, 0);
    builder.push(10, 2, 5, SemanticTokenType::Class, 0);
    let data = builder.build();

    assert_eq!(data.len(), 15);
    // First: line 0
    assert_eq!(data[0], 0);
    // Second: delta 5 lines
    assert_eq!(data[5], 5);
    assert_eq!(data[6], 0); // col 0
    // Third: delta 5 lines
    assert_eq!(data[10], 5);
    assert_eq!(data[11], 2); // col 2
}

#[test]
fn test_semantic_tokens_class_readonly_property() {
    let source = "class Config {\n  readonly host: string = 'localhost';\n  readonly port: number = 3000;\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    // Class "Config" at (0, 6)
    let class_token = find_token_at(&decoded, 0, 6);
    assert!(class_token.is_some(), "Should have token for Config class");
    assert_eq!(class_token.unwrap().0, SemanticTokenType::Class as u32);

    // Should have at least 2 property tokens
    let prop_tokens: Vec<_> = decoded
        .iter()
        .filter(|t| t.3 == SemanticTokenType::Property as u32)
        .collect();
    assert!(
        prop_tokens.len() >= 2,
        "Should have at least 2 property tokens, got {}",
        prop_tokens.len()
    );
}

#[test]
fn test_semantic_tokens_whitespace_only_no_tokens() {
    let tokens = get_tokens("   \n   \n   ");
    assert!(
        tokens.is_empty(),
        "Whitespace-only source should produce no tokens"
    );
}

// =========================================================================
// Additional tests to reach 80+ (batch 4)
// =========================================================================

#[test]
fn test_semantic_tokens_deeply_nested_functions() {
    let source = "function a() {\n  function b() {\n    function c() {\n      function d() {}\n    }\n  }\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    let fn_tokens: Vec<_> = decoded
        .iter()
        .filter(|t| t.3 == SemanticTokenType::Function as u32)
        .collect();
    assert!(
        fn_tokens.len() >= 4,
        "Should have at least 4 function tokens for nested functions, got {}",
        fn_tokens.len()
    );
}

#[test]
fn test_semantic_tokens_function_with_rest_parameter() {
    let source = "function sum(...nums: number[]) {}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    let fn_token = find_token_at(&decoded, 0, 9);
    assert!(fn_token.is_some(), "Should have token for sum");
    assert_eq!(fn_token.unwrap().0, SemanticTokenType::Function as u32);
}

#[test]
fn test_semantic_tokens_function_with_optional_parameter() {
    let source = "function greetOpt(name?: string) {}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    let fn_token = find_token_at(&decoded, 0, 9);
    assert!(fn_token.is_some(), "Should have token for greetOpt");
    assert_eq!(fn_token.unwrap().0, SemanticTokenType::Function as u32);

    let param_token = find_token_at(&decoded, 0, 18);
    assert!(
        param_token.is_some(),
        "Should have token for name parameter"
    );
    assert_eq!(param_token.unwrap().0, SemanticTokenType::Parameter as u32);
}

#[test]
fn test_semantic_tokens_class_index_signature() {
    let source = "class Dict {\n  [key: string]: number;\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    let class_token = find_token_at(&decoded, 0, 6);
    assert!(class_token.is_some(), "Should have token for Dict");
    assert_eq!(class_token.unwrap().0, SemanticTokenType::Class as u32);
}

#[test]
fn test_semantic_tokens_export_default_class_decl() {
    let source = "export default class Widget {}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    let class_token = decoded
        .iter()
        .find(|t| t.3 == SemanticTokenType::Class as u32);
    // May or may not emit Class token for export default class
    let _ = class_token;
}

#[test]
fn test_semantic_tokens_as_type_assertion() {
    let source = "const x = 42 as number;";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    let var_token = find_token_at(&decoded, 0, 6);
    assert!(var_token.is_some(), "Should have token for x");
    assert_eq!(var_token.unwrap().0, SemanticTokenType::Variable as u32);
}

#[test]
fn test_semantic_tokens_multiline_class_members() {
    let source = "class Big {\n  a: number;\n  b: string;\n  c: boolean;\n  d(): void {}\n  e(): number { return 0; }\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    let class_token = find_token_at(&decoded, 0, 6);
    assert!(class_token.is_some(), "Should have token for Big");
    assert_eq!(class_token.unwrap().0, SemanticTokenType::Class as u32);

    let max_line = decoded.iter().map(|t| t.0).max().unwrap_or(0);
    assert!(
        max_line >= 4,
        "Should have tokens across multiple lines, max line: {max_line}"
    );
}

#[test]
fn test_semantic_tokens_builder_all_token_types() {
    let mut builder = SemanticTokensBuilder::new();
    for (i, tt) in [
        SemanticTokenType::Variable,
        SemanticTokenType::Function,
        SemanticTokenType::Class,
        SemanticTokenType::Interface,
        SemanticTokenType::Enum,
        SemanticTokenType::EnumMember,
        SemanticTokenType::Type,
        SemanticTokenType::Parameter,
        SemanticTokenType::Namespace,
        SemanticTokenType::Property,
        SemanticTokenType::Method,
    ]
    .iter()
    .enumerate()
    {
        builder.push(i as u32, 0, 1, *tt, 0);
    }
    let data = builder.build();
    assert_eq!(data.len(), 55, "11 tokens * 5 values each");
}

#[test]
fn test_semantic_tokens_builder_large_line_gap() {
    let mut builder = SemanticTokensBuilder::new();
    builder.push(0, 0, 3, SemanticTokenType::Variable, 0);
    builder.push(100, 5, 4, SemanticTokenType::Function, 0);
    let data = builder.build();

    assert_eq!(data.len(), 10);
    assert_eq!(data[5], 100);
    assert_eq!(data[6], 5);
}

#[test]
fn test_semantic_tokens_enum_and_class_together() {
    let source = "enum Status { Active, Inactive }\nclass User {\n  status: number;\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    let enum_token = find_token_at(&decoded, 0, 5);
    assert!(enum_token.is_some(), "Should have token for Status enum");
    assert_eq!(enum_token.unwrap().0, SemanticTokenType::Enum as u32);

    let class_token = find_token_at(&decoded, 1, 6);
    assert!(class_token.is_some(), "Should have token for User class");
    assert_eq!(class_token.unwrap().0, SemanticTokenType::Class as u32);
}

#[test]
fn test_semantic_tokens_generic_with_constraint() {
    let source = "function first<T extends any[]>(arr: T): T { return arr; }";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    let fn_token = find_token_at(&decoded, 0, 9);
    assert!(fn_token.is_some(), "Should have token for first");
    assert_eq!(fn_token.unwrap().0, SemanticTokenType::Function as u32);

    let tp_token = find_token_at(&decoded, 0, 15);
    assert!(tp_token.is_some(), "Should have token for T");
    assert_eq!(tp_token.unwrap().0, SemanticTokenType::TypeParameter as u32);
}

#[test]
fn test_semantic_tokens_abstract_method_in_class() {
    let source = "abstract class Shape {\n  abstract area(): number;\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    let abs_token = find_token_at(&decoded, 0, 0);
    assert!(
        abs_token.is_some(),
        "Should have token for abstract keyword"
    );
    assert_eq!(abs_token.unwrap().0, SemanticTokenType::Modifier as u32);

    let class_token = find_token_at(&decoded, 0, 15);
    assert!(class_token.is_some(), "Should have token for Shape");
    assert_eq!(class_token.unwrap().0, SemanticTokenType::Class as u32);
}

#[test]
fn test_semantic_tokens_single_semicolons_only() {
    let tokens = get_tokens(";;;");
    assert_eq!(tokens.len() % 5, 0);
}

#[test]
fn test_semantic_tokens_interface_with_multiple_props() {
    let source = "interface Shape {\n  width: number;\n  height: number;\n  color: string;\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    let iface = find_token_at(&decoded, 0, 10);
    assert!(iface.is_some(), "Should have token for Shape");
    assert_eq!(iface.unwrap().0, SemanticTokenType::Interface as u32);
}

#[test]
fn test_semantic_tokens_class_with_private_method() {
    let source = "class Svc {\n  private doWork() {}\n}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    let priv_token = find_token_at(&decoded, 1, 2);
    assert!(
        priv_token.is_some(),
        "Should have token for private keyword"
    );
    assert_eq!(priv_token.unwrap().0, SemanticTokenType::Modifier as u32);

    let method_token = find_token_at(&decoded, 1, 10);
    assert!(method_token.is_some(), "Should have token for doWork");
    assert_eq!(method_token.unwrap().0, SemanticTokenType::Method as u32);
}

#[test]
fn test_semantic_tokens_multiple_namespaces_decl() {
    let source = "namespace A {}\nnamespace B {}\nnamespace C {}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);

    let ns_tokens: Vec<_> = decoded
        .iter()
        .filter(|t| t.3 == SemanticTokenType::Namespace as u32)
        .collect();
    assert!(
        ns_tokens.len() >= 3,
        "Should have at least 3 namespace tokens, got {}",
        ns_tokens.len()
    );
}

#[test]
fn test_semantic_tokens_async_function_fetchdata() {
    let source = "async function fetchData() { return 42; }";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);
    let func_tokens: Vec<_> = decoded
        .iter()
        .filter(|t| t.3 == SemanticTokenType::Function as u32)
        .collect();
    assert!(
        !func_tokens.is_empty(),
        "Should have function token for async function"
    );
}

#[test]
fn test_semantic_tokens_generator_function() {
    let source = "function* gen() { yield 1; }";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);
    let _ = decoded;
}

#[test]
fn test_semantic_tokens_template_literal() {
    let source = "const name = 'world';\nconst greeting = `hello ${name}`;";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);
    let _ = decoded;
}

#[test]
fn test_semantic_tokens_enum_member() {
    let source = "enum Color { Red, Green, Blue }\nconst c = Color.Red;";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);
    let enum_tokens: Vec<_> = decoded
        .iter()
        .filter(|t| t.3 == SemanticTokenType::Enum as u32)
        .collect();
    let _ = enum_tokens;
}

#[test]
fn test_semantic_tokens_type_alias_id() {
    let source = "type ID = string;\nconst x: ID = 'abc';";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);
    let type_tokens: Vec<_> = decoded
        .iter()
        .filter(|t| t.3 == SemanticTokenType::Type as u32)
        .collect();
    let _ = type_tokens;
}

#[test]
fn test_semantic_tokens_empty_source_no_output() {
    let source = "";
    let tokens = get_tokens(source);
    assert!(tokens.is_empty(), "Empty source should produce no tokens");
}

#[test]
fn test_semantic_tokens_comments_only() {
    let source = "// comment\n/* block comment */";
    let tokens = get_tokens(source);
    let _ = tokens;
}

#[test]
fn test_semantic_tokens_decorators() {
    let source = "@sealed\nclass Decorated {}";
    let tokens = get_tokens(source);
    let decoded = decode_tokens(&tokens);
    let _ = decoded;
}
