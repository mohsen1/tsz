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

#[test]
fn test_parser_interface_with_call_and_construct() {
    // This is a common pattern for class constructors
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        r#"interface FooConstructor {
            new (): Foo;
            prototype: Foo;
        }
        interface Foo {
            (): string;
            bar(key: string): string;
        }"#
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
fn test_parser_type_literal_with_call_signature() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Fn = { (): void; message: string }".to_string(),
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
fn test_parser_accessor_signature_in_type() {
    // Accessor signatures in type context (allowed syntactically)
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type A = { get foo(): number; set foo(v: number); }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

