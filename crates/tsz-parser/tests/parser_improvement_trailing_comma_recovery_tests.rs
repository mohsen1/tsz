//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — trailing comma recovery.

use crate::parser::test_fixture::parse_source;

#[test]
fn test_variable_list_trailing_comma_reports_at_comma() {
    let source = "var a,\nreturn;";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let comma_pos = source.find(',').expect("comma position") as u32;

    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 1009
                && diag.start == comma_pos
                && diag.message == "Trailing comma not allowed."
        }),
        "Expected TS1009 at the trailing comma, got {diagnostics:?}"
    );
}

#[test]
fn test_trailing_comma_in_parameters() {
    // Trailing commas should be allowed in function parameters
    let source = r"
function foo(
    a: number,
    b: string,
) {
    return a + b;
}
";
    let (parser, _root) = parse_source(source);

    // Should not emit any errors for trailing comma in parameters
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no errors for trailing comma in parameters, got {:?}",
        parser.get_diagnostics()
    );
}
