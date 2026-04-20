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

