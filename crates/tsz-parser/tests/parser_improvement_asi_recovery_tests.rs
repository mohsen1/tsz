//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — asi recovery.

use crate::parser::test_fixture::parse_source;

#[test]
fn test_asi_after_return() {
    // ASI (automatic semicolon insertion) should work after return
    let source = r"
function foo() {
    return
    42;
}
";
    let (parser, _root) = parse_source(source);

    // Should not emit TS1005 for missing semicolon after return with line break
    let ts1005_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1005)
        .count();
    assert_eq!(
        ts1005_count, 0,
        "Expected no TS1005 errors for ASI after return, got {ts1005_count}",
    );
}

#[test]
fn test_interface_extends_property_with_asi() {
    // 'extends' as a property name in interface with ASI (no semicolons)
    // Should NOT parse as conditional type
    let source = r"
interface JSONSchema4 {
  a?: number
  extends?: string | string[]
}
";
    let (parser, _root) = parse_source(source);

    let diags = parser.get_diagnostics();
    assert!(
        diags.is_empty(),
        "Expected no parser errors for 'extends' property with ASI, got {diags:?}",
    );
}
