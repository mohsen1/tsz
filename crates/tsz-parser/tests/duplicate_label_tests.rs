//! Tests for duplicate label detection (TS1114)

use crate::parser::state::ParserState;

#[test]
fn test_duplicate_label_nested() {
    let source = r"
target:
target:
while (true) {}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should have one TS1114 error for duplicate label
    let errors: Vec<_> = parser
        .parse_diagnostics
        .iter()
        .filter(|d| d.code == 1114)
        .collect();
    assert_eq!(
        errors.len(),
        1,
        "Expected 1 TS1114 error for nested duplicate labels"
    );
}

#[test]
fn test_duplicate_label_sequential_allowed() {
    let source = r"
target:
while (true) {}

target:
while (true) {}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should have NO TS1114 error - sequential labels are allowed
    let errors: Vec<_> = parser
        .parse_diagnostics
        .iter()
        .filter(|d| d.code == 1114)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no TS1114 error for sequential labels"
    );
}

#[test]
fn test_duplicate_label_function_scoped() {
    let source = r"
target:
while (true) {
  function f() {
    target:
    while (true) {}
  }
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should have NO TS1114 error - labels in different function scopes are allowed
    let errors: Vec<_> = parser
        .parse_diagnostics
        .iter()
        .filter(|d| d.code == 1114)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no TS1114 error for function-scoped labels"
    );
}

#[test]
fn test_duplicate_label_arrow_function_scoped() {
    let source = r"
target:
for (;;) {
  const f = () => {
    target:
    while (true) {}
  };
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should have NO TS1114 error - labels in arrow function have separate scope
    let errors: Vec<_> = parser
        .parse_diagnostics
        .iter()
        .filter(|d| d.code == 1114)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no TS1114 error for arrow function-scoped labels"
    );
}

#[test]
fn test_duplicate_label_in_nested_blocks() {
    let source = r"
{
  target: while (true) {}
}
{
  target: while (true) {}
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should have NO TS1114 error - labels in separate blocks are sequential
    let errors: Vec<_> = parser
        .parse_diagnostics
        .iter()
        .filter(|d| d.code == 1114)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no TS1114 error for labels in separate blocks"
    );
}
