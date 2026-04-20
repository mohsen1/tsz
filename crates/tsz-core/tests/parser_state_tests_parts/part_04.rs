#[test]
fn test_parser_class_extends_call() {
    // Class extends a mixin call
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Child extends Mixin(Parent) {}".to_string(),
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
fn test_parser_class_extends_property_access() {
    // Class extends a property access
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Child extends Base.Parent {}".to_string(),
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
fn test_parser_decorator_class() {
    let mut parser = ParserState::new("test.ts".to_string(), "@Component class Foo {}".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_decorator_with_call() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "@Component({ selector: 'app' }) class AppComponent {}".to_string(),
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
fn test_parser_multiple_decorators() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "@Component @Injectable class Service {}".to_string(),
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
fn test_parser_decorator_abstract_class() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "@Serializable abstract class Base {}".to_string(),
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
fn test_parser_class_extends_and_implements() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Foo extends Base implements A, B, C {}".to_string(),
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
fn test_parser_abstract_class() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "abstract class Base { abstract method(): void; }".to_string(),
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
fn test_parser_abstract_class_in_iife() {
    // This was causing crashes before - abstract class inside IIFE
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "(function() { abstract class Foo {} return Foo; })()".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    // Should parse without crashing
}

#[test]
fn test_parser_get_accessor() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Foo { get value(): number { return 42; } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

