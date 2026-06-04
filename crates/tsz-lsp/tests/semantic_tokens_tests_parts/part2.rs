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
