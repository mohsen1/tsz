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
    let mut parser = ParserState::new("test.ts".to_string(), "let x = obj?.<T>(value)".to_string());
    let root = parser.parse_source_file();

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

