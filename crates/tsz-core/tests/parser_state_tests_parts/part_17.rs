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
    let mut parser = ParserState::new("test.ts".to_string(), "let x = a ?? b ?? c".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
}

