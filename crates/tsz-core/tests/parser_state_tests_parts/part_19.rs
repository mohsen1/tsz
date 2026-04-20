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

