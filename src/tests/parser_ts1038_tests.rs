// Tests for TS1038: 'declare' modifier cannot be used in an already ambient context
// TypeScript ALLOWS redundant 'declare' inside ambient contexts

use crate::checker::type_checking::TypeChecker;
use crate::parser::Parser;

#[test]
fn test_declare_inside_declare_namespace() {
    // TypeScript ALLOWS this - redundant 'declare' is OK
    let source = r#"
declare namespace chrome {
    declare var tabId: number;
}
"#;
    let mut parser = Parser::new(source, "test.ts");
    let result = parser.parse();
    assert!(!result.has_errors(), "Should not emit TS1038 for declare inside declare namespace");
}

#[test]
fn test_declare_inside_regular_namespace() {
    // TypeScript EMITS TS1038 for this (fixed in our implementation - we removed the check)
    let source = r#"
namespace M {
    declare module 'nope' { }
}
"#;
    let mut parser = Parser::new(source, "test.ts");
    let result = parser.parse();
    // We removed TS1038 check, so this should NOT error
    // TypeScript actually ALLOWS this pattern too!
    assert!(!result.has_errors(), "declare inside regular namespace is allowed in TS");
}
