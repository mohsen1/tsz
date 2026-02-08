//! Integration tests for misspelled-keyword diagnostics.
//!
//! These exercise the full parse pipeline (scanner → parser → diagnostics)
//! to verify that `parse_error_for_missing_semicolon_after` produces the
//! correct error codes and messages.

use crate::parser::state::ParserState;

fn parse_and_collect_codes(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();
    parser
        .parse_diagnostics
        .iter()
        .map(|d| (d.code, d.message.clone()))
        .collect()
}

fn has_diagnostic(diags: &[(u32, String)], code: u32, substring: &str) -> bool {
    diags
        .iter()
        .any(|(c, msg)| *c == code && msg.contains(substring))
}

// =====================================================================
// TS1435: "Unknown keyword or identifier. Did you mean '{0}'?"
// =====================================================================

#[test]
fn ts1435_misspelled_async() {
    let diags = parse_and_collect_codes("asynd function foo() {}");
    assert!(
        has_diagnostic(&diags, 1435, "async"),
        "expected TS1435 suggesting 'async', got {diags:?}"
    );
}

#[test]
fn ts1435_misspelled_async_prefix() {
    let diags = parse_and_collect_codes("sasync function foo() {}");
    assert!(
        has_diagnostic(&diags, 1435, "async"),
        "expected TS1435 suggesting 'async', got {diags:?}"
    );
}

#[test]
fn ts1435_misspelled_class() {
    let diags = parse_and_collect_codes("clasd Foo {}");
    assert!(
        has_diagnostic(&diags, 1435, "class"),
        "expected TS1435 suggesting 'class', got {diags:?}"
    );
}

#[test]
fn ts1435_misspelled_const() {
    let diags = parse_and_collect_codes("consd x = 1;");
    assert!(
        has_diagnostic(&diags, 1435, "const"),
        "expected TS1435 suggesting 'const', got {diags:?}"
    );
}

#[test]
fn ts1435_misspelled_function() {
    let diags = parse_and_collect_codes("functiond foo() {}");
    assert!(
        has_diagnostic(&diags, 1435, "function"),
        "expected TS1435 suggesting 'function', got {diags:?}"
    );
}

#[test]
fn ts1435_misspelled_interface() {
    let diags = parse_and_collect_codes("interfaced Foo {}");
    assert!(
        has_diagnostic(&diags, 1435, "interface"),
        "expected TS1435 suggesting 'interface', got {diags:?}"
    );
}

#[test]
fn ts1435_misspelled_var() {
    let diags = parse_and_collect_codes("vard x = 1;");
    assert!(
        has_diagnostic(&diags, 1435, "var"),
        "expected TS1435 suggesting 'var', got {diags:?}"
    );
}

#[test]
fn ts1435_misspelled_let() {
    let diags = parse_and_collect_codes("letd x = 1;");
    assert!(
        has_diagnostic(&diags, 1435, "let"),
        "expected TS1435 suggesting 'let', got {diags:?}"
    );
}

#[test]
fn ts1435_misspelled_declare() {
    let diags = parse_and_collect_codes("declared const x: 1;");
    assert!(
        has_diagnostic(&diags, 1435, "declare"),
        "expected TS1435 suggesting 'declare', got {diags:?}"
    );
}

#[test]
fn ts1435_misspelled_type() {
    let diags = parse_and_collect_codes("typed T = {}");
    assert!(
        has_diagnostic(&diags, 1435, "type"),
        "expected TS1435 suggesting 'type', got {diags:?}"
    );
}

// =====================================================================
// TS1435 via space suggestion (keyword concatenation)
// =====================================================================

#[test]
fn ts1435_declareconst_space() {
    let diags = parse_and_collect_codes("declareconst x;");
    assert!(
        has_diagnostic(&diags, 1435, "declare const"),
        "expected TS1435 suggesting 'declare const', got {diags:?}"
    );
}

#[test]
fn ts1435_interface_my_interface_space() {
    let diags = parse_and_collect_codes("interfaceMyInterface { }");
    assert!(
        has_diagnostic(&diags, 1435, "interface MyInterface"),
        "expected TS1435 suggesting 'interface MyInterface', got {diags:?}"
    );
}

// =====================================================================
// TS1434: "Unexpected keyword or identifier." (fallback)
// =====================================================================

#[test]
fn ts1434_unknown_identifier_after_misspelled_class() {
    // "clasd MyClass2 {}" → TS1435 on "clasd", TS1434 on "MyClass2"
    let diags = parse_and_collect_codes("clasd MyClass2 {}");
    assert!(
        has_diagnostic(&diags, 1435, "class"),
        "expected TS1435 on 'clasd', got {diags:?}"
    );
    assert!(
        has_diagnostic(&diags, 1434, "Unexpected keyword or identifier"),
        "expected TS1434 on 'MyClass2', got {diags:?}"
    );
}

// =====================================================================
// Special keyword handling
// =====================================================================

#[test]
fn ts1005_type_equals_expected() {
    // "type type;" triggers '=' expected
    let diags = parse_and_collect_codes("type type;");
    assert!(
        has_diagnostic(&diags, 1005, "'=' expected"),
        "expected TS1005 '=' expected for 'type type;', got {diags:?}"
    );
}

#[test]
fn ts1005_semicolon_for_non_identifier() {
    // Non-identifier expression followed by something unexpected → TS1005
    let diags = parse_and_collect_codes("123 abc");
    assert!(
        has_diagnostic(&diags, 1005, "';' expected"),
        "expected TS1005 for numeric literal, got {diags:?}"
    );
}

// =====================================================================
// Correct code is NOT flagged
// =====================================================================

#[test]
fn no_error_for_valid_declarations() {
    let cases = [
        "async function foo() {}",
        "class Foo {}",
        "const x = 1;",
        "function foo() {}",
        "interface Foo {}",
        "let x = 1;",
        "var x = 1;",
    ];
    for input in cases {
        let diags = parse_and_collect_codes(input);
        let has_1434_or_1435 = diags.iter().any(|(c, _)| *c == 1434 || *c == 1435);
        assert!(
            !has_1434_or_1435,
            "valid input '{input}' should not produce TS1434/TS1435, got {diags:?}"
        );
    }
}
