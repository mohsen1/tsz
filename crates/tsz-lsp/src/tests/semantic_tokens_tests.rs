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
